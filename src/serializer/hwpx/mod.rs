//! HWPX(ZIP+XML) 직렬화 모듈 — `parser::hwpx`의 역방향.
//!
//! ## 단계 (#182)
//! - Stage 0 (완료): 기반 공사 — SerializeContext, IrDiff 하네스, canonical_defaults
//! - Stage 1: header.xml IR 기반 동적 생성
//! - Stage 2: section.xml 동적화 + charPrIDRef 매핑
//! - Stage 3: 표(Table)
//! - Stage 4: 그림(Picture) + BinData
//! - Stage 5: 도형·필드 + 대형 실문서 스모크

pub mod canonical_defaults;
pub mod content;
pub mod context;
pub mod field;
pub mod fixtures;
pub mod form;
pub mod header;
pub mod master_page;
pub mod package_check;
pub mod picture;
pub mod roundtrip;
pub mod section;
pub mod shape;
pub mod static_assets;
pub mod table;
pub mod utils;
pub mod writer;

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::model::document::{Document, HWP5_ORIGIN_HWPX_MARKER_PATH};

use super::SerializeError;
use content::BinDataEntry as ContentBinDataEntry;
use context::SerializeContext;
use writer::HwpxZipWriter;

/// Document IR을 HWPX(ZIP+XML) 바이트로 직렬화한다.
///
/// Stage 0 이후: 빈 문서 특수 분기를 제거하고 **항상 동적 경로**를 탄다.
/// `SerializeContext`가 1-pass 스캔으로 ID 풀을 구성하고, 각 writer가 동일 컨텍스트를
/// 참조한다. 직렬화 종료 시 `assert_all_refs_resolved()`가 미등록 참조를 단언한다.
pub fn serialize_hwpx(doc: &Document) -> Result<Vec<u8>, SerializeError> {
    use static_assets::*;

    // 1-pass: ID 풀 구성
    let mut ctx = SerializeContext::collect_from_document(doc);

    let mut z = HwpxZipWriter::new();

    // 1. mimetype (반드시 최초 엔트리, STORED, extra field 없음)
    z.write_stored("mimetype", b"application/hwp+zip")?;

    // 2. version.xml — 원본 보존 우선 (없으면 하드코딩 상수).
    //    하드코딩 상수는 Windows/특정 빌드 고정값이라 한컴 변환본의 실제 플랫폼
    //    버전을 덮어쓴다. 원본 보조 엔트리가 있으면 그대로 출력한다.
    z.write_deflated(
        "version.xml",
        doc.hwpx_aux_entry("version.xml")
            .unwrap_or_else(|| VERSION_XML.as_bytes()),
    )?;

    // 3. Contents/header.xml — Stage 1 동적 생성 (IR 기반)
    let header_xml = header::write_header(doc, &ctx)?;
    z.write_deflated("Contents/header.xml", &header_xml)?;

    // 4. Contents/section{N}.xml — 실제 섹션만큼, 없으면 0개
    let section_hrefs: Vec<String> = (0..doc.sections.len())
        .map(|i| format!("Contents/section{}.xml", i))
        .collect();
    for (i, sec) in doc.sections.iter().enumerate() {
        let xml = section::write_section(sec, doc, i, &mut ctx)?;
        z.write_deflated(&section_hrefs[i], &xml)?;
    }

    // 4b. Contents/masterpage{N}.xml — 바탕쪽 (전 섹션 누적 전역 인덱스).
    //     id/href 의 인덱스는 section.rs 의 idRef 인덱스와 동일 규칙(전역 누적)이라
    //     별도 공유 상태 없이 정합한다.
    let mut master_items: Vec<(String, String)> = Vec::new();
    let mut mp_global = 0usize;
    for sec in &doc.sections {
        for mp in &sec.section_def.master_pages {
            let id = format!("masterpage{}", mp_global);
            let href = format!("Contents/masterpage{}.xml", mp_global);
            let xml = master_page::render_master_page_xml(mp, &id, &mut ctx);
            z.write_deflated(&href, xml.as_bytes())?;
            master_items.push((id, href));
            mp_global += 1;
        }
    }

    // 5. Preview/PrvText.txt + Preview/PrvImage.png — 원본 보존 우선.
    //    하드코딩 상수는 빈/placeholder 미리보기라 원본 썸네일·미리보기 텍스트를
    //    잃는다. 보조 엔트리가 있으면 원본을 그대로 출력한다.
    z.write_deflated(
        "Preview/PrvText.txt",
        doc.hwpx_aux_entry("Preview/PrvText.txt")
            .unwrap_or(PRV_TEXT),
    )?;
    z.write_deflated(
        "Preview/PrvImage.png",
        doc.hwpx_aux_entry("Preview/PrvImage.png")
            .unwrap_or(PRV_IMAGE_PNG),
    )?;

    // 6. settings.xml — 원본 보존 우선.
    //    하드코딩 상수는 PrintInfo(확대/인쇄 설정) 등을 빠뜨린다.
    z.write_deflated(
        "settings.xml",
        doc.hwpx_aux_entry("settings.xml")
            .unwrap_or_else(|| SETTINGS_XML.as_bytes()),
    )?;

    // 7. META-INF/container.rdf — header + every section part.
    // Hancom uses this RDF graph alongside content.hpf; a stale one-section
    // RDF makes multi-section documents fail to open even when the ZIP and
    // content.hpf contain every section.
    let container_rdf = write_container_rdf(&section_hrefs);
    z.write_deflated("META-INF/container.rdf", container_rdf.as_bytes())?;

    // 8. BinData ZIP 엔트리 (Stage 4)
    //    `ctx.bin_data_map` 의 엔트리 순서대로 실제 바이너리를 ZIP에 추가.
    //    3-way 단언(binaryItemIDRef ↔ manifest ↔ ZIP entry) 의 1차 출력 지점.
    let bin_entries = ctx.bin_data_entries();
    let mut zip_bin_entries: HashSet<String> = HashSet::new();
    for entry in &bin_entries {
        // 외부 참조(isEmbeded=0)는 ZIP 엔트리가 없다 — manifest 항목만 방출 (#1891).
        if !entry.is_embedded {
            continue;
        }
        let data = doc
            .bin_data_content
            .iter()
            .find(|b| b.id == entry.bin_data_id)
            .ok_or_else(|| {
                SerializeError::XmlError(format!(
                    "BinDataContent 누락: bin_data_id={}",
                    entry.bin_data_id
                ))
            })?;
        // [OLE size prefix 복원 2026-08-13] HWPX 파서는 내부 OLE 엔트리의 선두 4-byte LE
        // size prefix 를 벗겨 IR 에 담는다(normalize_ole_bytes). 저장할 때 도로 붙이지
        // 않으면 원본보다 정확히 4바이트 짧은 파일이 나와 한컴이 OLE 를 못 읽는다
        // (실측 273,924 → 273,920). HWP5 라이터(cfb_writer.rs)는 이미 복원한다.
        let bytes = data.data.load();
        const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        let needs_prefix = entry.href.to_ascii_lowercase().ends_with(".ole")
            && bytes.len() >= 8
            && bytes[..8] == CFB_MAGIC;
        if needs_prefix {
            let mut out = Vec::with_capacity(bytes.len() + 4);
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
            z.write_deflated(&entry.href, &out)?;
        } else {
            z.write_deflated(&entry.href, &bytes)?;
        }
        zip_bin_entries.insert(entry.href.clone());
    }

    // 9. Contents/content.hpf — 항상 동적 경로 + BinData 매니페스트 엔트리
    let content_bin_entries: Vec<ContentBinDataEntry> = bin_entries
        .iter()
        // [차트 파트 2026-08-13] Chart/chart{N}.xml 은 manifest 에 올리지 않는다 —
        // 한컴 자체 저장 파일이 그렇고(샘플 전수), 등록하면 원본과 매니페스트가 갈린다.
        .filter(|e| !e.href.starts_with("Chart/"))
        .map(|e| ContentBinDataEntry {
            id: e.manifest_id.clone(),
            href: e.href.clone(),
            media_type: e.media_type.clone(),
            is_embedded: e.is_embedded,
        })
        .collect();
    let content_hpf = content::write_content_hpf(
        &section_hrefs,
        &content_bin_entries,
        &master_items,
        doc.hwpx_aux_entry("Contents/content.hpf"),
    )?;
    z.write_deflated("Contents/content.hpf", &content_hpf)?;

    // 10. META-INF/container.xml
    z.write_deflated("META-INF/container.xml", META_INF_CONTAINER_XML.as_bytes())?;

    // 11. META-INF/manifest.xml
    z.write_deflated("META-INF/manifest.xml", META_INF_MANIFEST_XML.as_bytes())?;

    // HWP5-origin HWPX marker — HWP5에서 HWPX로 export한 산출물은 HWPX 컨테이너라도
    // lineSeg 부재/pagination 시멘틱을 HWP5 원본처럼 해석해야 자기정합한다.
    if let Some(marker) = doc.hwpx_aux_entry(HWP5_ORIGIN_HWPX_MARKER_PATH) {
        z.write_deflated(HWP5_ORIGIN_HWPX_MARKER_PATH, marker)?;
    }

    // 변경 추적 사이드카 — 마크·레코드를 JSON 으로 (스펙 track-changes.md).
    // 우리 편집기 왕복 보존용. 한컴 표준 태그는 v2 — 이 파일은 한컴이 무시한다.
    let track_json = crate::serializer::track_sidecar::build_track_sidecar(doc);
    if let Some(json) = track_json {
        z.write_deflated(crate::model::document::TRACK_SIDECAR_PATH, json.as_bytes())?;
    }

    // 참조 정합성 단언 (Stage 1+)
    ctx.assert_all_refs_resolved()?;

    // 3-way BinData 단언 (Stage 4):
    //   - ctx.bin_data_map 의 manifest_id/href 집합
    //   - content.hpf opf:item (위에서 content_bin_entries 로 생성됨, 집합 동일)
    //   - ZIP entry (위에서 zip_bin_entries 로 기록됨)
    // 세 집합이 동일해야 한컴이 바인딩 오류 없이 그림을 표시함.
    assert_bin_data_3way(&bin_entries, &zip_bin_entries)?;

    z.finish()
}

fn write_container_rdf(section_hrefs: &[String]) -> String {
    const PKG_NS: &str = "http://www.hancom.co.kr/hwpml/2016/meta/pkg#";

    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#);
    out.push_str(r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#);
    out.push_str(r#"<rdf:Description rdf:about="">"#);
    let _ = write!(
        out,
        r#"<ns0:hasPart xmlns:ns0="{PKG_NS}" rdf:resource="Contents/header.xml"/>"#
    );
    out.push_str(r#"</rdf:Description>"#);
    out.push_str(r#"<rdf:Description rdf:about="Contents/header.xml">"#);
    let _ = write!(out, r#"<rdf:type rdf:resource="{PKG_NS}HeaderFile"/>"#);
    out.push_str(r#"</rdf:Description>"#);

    for href in section_hrefs {
        out.push_str(r#"<rdf:Description rdf:about="">"#);
        let _ = write!(
            out,
            r#"<ns0:hasPart xmlns:ns0="{PKG_NS}" rdf:resource="{href}"/>"#
        );
        out.push_str(r#"</rdf:Description>"#);
        let _ = write!(out, r#"<rdf:Description rdf:about="{href}">"#);
        let _ = write!(out, r#"<rdf:type rdf:resource="{PKG_NS}SectionFile"/>"#);
        out.push_str(r#"</rdf:Description>"#);
    }

    out.push_str(r#"<rdf:Description rdf:about="">"#);
    let _ = write!(out, r#"<rdf:type rdf:resource="{PKG_NS}Document"/>"#);
    out.push_str(r#"</rdf:Description>"#);
    out.push_str(r#"</rdf:RDF>"#);
    out
}

/// 3-way BinData 동기화 단언: `ctx.bin_data_entries()`, content.hpf manifest,
/// ZIP entry 의 href 집합이 모두 일치하는지 확인.
/// 외부 참조(isEmbeded=0) 항목은 ZIP 엔트리가 없는 것이 정상이므로 제외한다 (#1891).
fn assert_bin_data_3way(
    bin_entries: &[context::BinDataEntry],
    zip_entries: &HashSet<String>,
) -> Result<(), SerializeError> {
    let ctx_hrefs: HashSet<String> = bin_entries
        .iter()
        .filter(|e| e.is_embedded)
        .map(|e| e.href.clone())
        .collect();
    if ctx_hrefs != *zip_entries {
        let missing_zip: Vec<_> = ctx_hrefs.difference(zip_entries).cloned().collect();
        let orphan_zip: Vec<_> = zip_entries.difference(&ctx_hrefs).cloned().collect();
        return Err(SerializeError::XmlError(format!(
            "3-way BinData 불일치: ctx(href) vs zip_entries — ctx에만 있음: {:?}, zip에만 있음: {:?}",
            missing_zip, orphan_zip
        )));
    }
    Ok(())
}
