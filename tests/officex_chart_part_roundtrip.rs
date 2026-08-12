//! [2026-08-13 차트 소멸] HWPX OOXML 차트 파트 왕복 보존 핀.
//!
//! 결함: HWPX 는 차트를 `Chart/chart{N}.xml` **별도 zip 파트**로 담고 본문에서
//! `chartIDRef` 로 참조하는데, 직렬화기가 이를 다른 임베드와 똑같이
//! `BinData/image60001.ooxml_chart` 로 써서 — 열었다 저장하는 것만으로 한컴이 찾는
//! 차트 파트가 사라졌다(샘플 28/28 소멸). 참조도 `hp:switch`(chartIDRef) → `hp:ole`
//! 로 강등돼 되찾을 길이 없었다. 덤으로 내부 OLE 의 선두 4바이트 size prefix 가
//! 복원되지 않아 원본보다 정확히 4바이트 짧은 파일이 나왔다(273,924 → 273,920).

use std::io::Read;

fn chart_parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip 열기");
    let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
    let mut out = Vec::new();
    for name in names {
        if !name.starts_with("Chart/") {
            continue;
        }
        let mut f = zip.by_name(&name).expect("파트 열기");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).expect("파트 읽기");
        out.push((name, buf));
    }
    out.sort();
    out
}

fn entry_len(bytes: &[u8], name: &str) -> Option<usize> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf.len())
}

fn roundtrip(path: &str) -> (Vec<u8>, Vec<u8>) {
    let orig = std::fs::read(path).unwrap_or_else(|e| panic!("{path} 읽기 실패: {e}"));
    let doc = rhwp::parser::parse_document(&orig).expect("파싱");
    let out = rhwp::serializer::hwpx::serialize_hwpx(&doc).expect("HWPX 직렬화");
    (orig, out)
}

/// 차트 파트가 이름·바이트 그대로 왕복해야 한다 — 우리는 차트 XML 을 재생성하지 않고
/// 통과시키므로 바이트 동일이 계약이다.
#[test]
fn chart_parts_survive_hwpx_roundtrip() {
    let mut checked = 0usize;
    let dir = std::path::Path::new("samples/chart");
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("hwpx") {
                continue;
            }
            let (orig, out) = roundtrip(p.to_str().unwrap());
            let a = chart_parts(&orig);
            if a.is_empty() {
                continue;
            }
            let b = chart_parts(&out);
            assert_eq!(
                a.iter()
                    .map(|(n, v)| (n.clone(), v.len()))
                    .collect::<Vec<_>>(),
                b.iter()
                    .map(|(n, v)| (n.clone(), v.len()))
                    .collect::<Vec<_>>(),
                "{:?}: 차트 파트가 왕복에서 사라지거나 바뀌었다",
                p
            );
            assert_eq!(a, b, "{:?}: 차트 XML 바이트가 변했다", p);
            checked += 1;
        }
    }
    assert!(checked >= 20, "차트 샘플을 못 찾았다(검사 {checked}건)");
}

/// 본문 참조가 `hp:switch`(case=ooxmlchart → chartIDRef)로 되돌아가야 한다.
/// hp:ole 단독으로 강등되면 한컴이 차트를 못 찾는다.
#[test]
fn chart_reference_stays_chart_id_ref() {
    let (_orig, out) = roundtrip("samples/chart/세로막대형/묶은세로막대형.hwpx");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&out)).expect("zip");
    let mut xml = String::new();
    zip.by_name("Contents/section0.xml")
        .expect("section0")
        .read_to_string(&mut xml)
        .expect("읽기");
    assert!(
        xml.contains("chartIDRef=\"Chart/chart1.xml\""),
        "chartIDRef 가 사라졌다(hp:ole 로 강등)"
    );
    assert!(
        xml.contains("hp:required-namespace=\"http://www.hancom.co.kr/hwpml/2016/ooxmlchart\""),
        "hp:switch case 네임스페이스가 없다"
    );
}

/// 차트 파트는 manifest(content.hpf)에 등록하지 않는다 — 한컴 자체 저장이 그렇다.
#[test]
fn chart_part_is_not_in_manifest() {
    let (orig, out) = roundtrip("samples/chart/세로막대형/묶은세로막대형.hwpx");
    for (bytes, tag) in [(&orig, "원본"), (&out, "왕복본")] {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        let mut hpf = String::new();
        zip.by_name("Contents/content.hpf")
            .expect("content.hpf")
            .read_to_string(&mut hpf)
            .expect("읽기");
        assert!(
            !hpf.contains("Chart/chart"),
            "{tag}: manifest 에 Chart 파트가 등록됐다"
        );
    }
}

/// 내부 OLE 의 선두 4바이트 size prefix 가 복원돼야 한다(원본과 같은 길이).
#[test]
fn internal_ole_keeps_size_prefix() {
    let (orig, out) = roundtrip("samples/chart/세로막대형/묶은세로막대형.hwpx");
    let a = entry_len(&orig, "BinData/ole1.ole").expect("원본 OLE");
    let b = entry_len(&out, "BinData/image1.OLE").expect("왕복 OLE");
    assert_eq!(a, b, "OLE 길이가 달라졌다(4바이트 prefix 소실)");
}
