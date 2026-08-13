//! [2026-08-13 차트 작성] 차트 XML 생성기 ↔ 파서 왕복 핀.
//!
//! 계약: `parse(build(spec))` 이 spec 을 그대로 돌려줘야 한다. 이게 성립해야
//! "삽입한 차트가 화면·PDF 에 제대로 그려진다"가 보장된다 — 렌더러는 파서 결과만 본다.
//! 또한 기존 차트를 편집(patch)해도 한컴 고유 서식이 살아남아야 한다.

use rhwp::ooxml_chart::writer::{build_chart_xml, patch_chart_xml, ChartSeriesSpec, ChartSpec};
use rhwp::ooxml_chart::{OoxmlChart, OoxmlChartType};

fn spec(kind: OoxmlChartType) -> ChartSpec {
    ChartSpec {
        style: String::new(),
        chart_type: kind,
        title: Some("분기 실적".to_string()),
        categories: vec![
            "1분기".into(),
            "2분기".into(),
            "3분기".into(),
            "4분기".into(),
        ],
        series: vec![
            ChartSeriesSpec {
                name: "서울".into(),
                values: vec![10.0, 20.5, 30.0, 15.25],
            },
            ChartSeriesSpec {
                name: "부산".into(),
                values: vec![5.0, 7.5, 9.0, 12.0],
            },
        ],
    }
}

/// 생성한 XML 을 우리 파서가 읽어 같은 데이터를 돌려준다(막대·꺾은선).
#[test]
fn generated_chart_parses_back_to_same_data() {
    for kind in [
        OoxmlChartType::Column,
        OoxmlChartType::Bar,
        OoxmlChartType::Line,
    ] {
        let s = spec(kind);
        let xml = build_chart_xml(&s);
        let parsed = OoxmlChart::parse(xml.as_bytes())
            .unwrap_or_else(|| panic!("{kind:?}: 생성한 XML 을 파서가 못 읽는다"));

        assert_eq!(parsed.title.as_deref(), Some("분기 실적"), "{kind:?}: 제목");
        assert_eq!(parsed.categories, s.categories, "{kind:?}: 항목");
        assert_eq!(parsed.series.len(), 2, "{kind:?}: 계열 수");
        for (i, want) in s.series.iter().enumerate() {
            assert_eq!(parsed.series[i].name, want.name, "{kind:?}: 계열{i} 이름");
            assert_eq!(parsed.series[i].values, want.values, "{kind:?}: 계열{i} 값");
        }
    }
}

/// 원형 차트는 계열 1개 — 나머지는 버리되 항목·값은 정확해야 한다.
#[test]
fn pie_chart_keeps_first_series_only() {
    let s = spec(OoxmlChartType::Pie);
    let xml = build_chart_xml(&s);
    let parsed = OoxmlChart::parse(xml.as_bytes()).expect("원형 파싱");
    assert_eq!(parsed.series.len(), 1);
    assert_eq!(parsed.series[0].name, "서울");
    assert_eq!(parsed.series[0].values, vec![10.0, 20.5, 30.0, 15.25]);
    assert_eq!(parsed.categories, s.categories);
}

/// 계열을 늘려도(템플릿보다 많아도) 전부 살아난다.
#[test]
fn more_series_than_template_still_parse() {
    let mut s = spec(OoxmlChartType::Column);
    for name in ["대구", "광주", "대전"] {
        s.series.push(ChartSeriesSpec {
            name: name.into(),
            values: vec![1.0, 2.0, 3.0, 4.0],
        });
    }
    let xml = build_chart_xml(&s);
    let parsed = OoxmlChart::parse(xml.as_bytes()).expect("파싱");
    assert_eq!(parsed.series.len(), 5, "계열 5개가 모두 살아야 한다");
    assert_eq!(parsed.series[4].name, "대전");
}

/// 기존(한컴 실물) 차트를 편집해도 파서가 새 데이터를 읽고, 서식 블록은 남는다.
#[test]
fn editing_existing_chart_keeps_formatting() {
    let orig = std::fs::read("samples/chart/세로막대형/묶은세로막대형.hwpx").expect("샘플");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&orig)).expect("zip");
    let mut xml = String::new();
    {
        use std::io::Read;
        zip.by_name("Chart/chart1.xml")
            .expect("차트 파트")
            .read_to_string(&mut xml)
            .expect("읽기");
    }
    let before_ext = xml.matches("c:extLst").count();
    let before_txpr = xml.matches("c:txPr").count();

    let s = spec(OoxmlChartType::Column);
    let edited = patch_chart_xml(&xml, &s);

    assert_eq!(
        edited.matches("c:extLst").count(),
        before_ext,
        "한컴 확장(extLst)이 편집으로 사라졌다"
    );
    assert_eq!(
        edited.matches("c:txPr").count(),
        before_txpr,
        "글꼴 서식(txPr)이 편집으로 사라졌다"
    );

    let parsed = OoxmlChart::parse(edited.as_bytes()).expect("편집본 파싱");
    assert_eq!(parsed.categories, s.categories);
    assert_eq!(parsed.series.len(), 2);
    assert_eq!(parsed.series[0].values, vec![10.0, 20.5, 30.0, 15.25]);
}

/// 생성한 차트가 실제로 **그려진다** — 렌더러가 빈 SVG 를 내면 삽입해도 안 보인다.
#[test]
fn generated_chart_renders_nonempty_svg() {
    for kind in [
        OoxmlChartType::Column,
        OoxmlChartType::Bar,
        OoxmlChartType::Line,
        OoxmlChartType::Pie,
    ] {
        let xml = build_chart_xml(&spec(kind));
        let chart = OoxmlChart::parse(xml.as_bytes()).expect("파싱");
        let svg = chart.render_svg(0.0, 0.0, 400.0, 300.0);
        assert!(
            svg.len() > 200,
            "{kind:?}: SVG 가 너무 짧다({}B)",
            svg.len()
        );
        // 데이터가 실제로 그려졌는지 — 항목 라벨이 SVG 텍스트로 나와야 한다
        assert!(
            svg.contains("1분기") || svg.contains("서울"),
            "{kind:?}: 데이터가 SVG 에 없다"
        );
    }
}

/// 값이 하나뿐인 차트, 계열 이름이 빈 문자열 등 가장자리 입력에서 깨지지 않는다.
#[test]
fn edge_inputs_do_not_panic() {
    let single = ChartSpec {
        style: String::new(),
        chart_type: OoxmlChartType::Column,
        title: None,
        categories: vec!["항목".into()],
        series: vec![ChartSeriesSpec {
            name: String::new(),
            values: vec![0.0],
        }],
    };
    let xml = build_chart_xml(&single);
    let parsed = OoxmlChart::parse(xml.as_bytes()).expect("단일 값 파싱");
    assert_eq!(parsed.series.len(), 1);
    assert_eq!(parsed.series[0].values, vec![0.0]);

    // 음수·큰 수
    let wide = ChartSpec {
        style: String::new(),
        chart_type: OoxmlChartType::Line,
        title: Some(String::new()),
        categories: vec!["a".into(), "b".into()],
        series: vec![ChartSeriesSpec {
            name: "s".into(),
            values: vec![-1234.5, 9_999_999.0],
        }],
    };
    let xml = build_chart_xml(&wide);
    let parsed = OoxmlChart::parse(xml.as_bytes()).expect("음수 파싱");
    assert_eq!(parsed.series[0].values, vec![-1234.5, 9_999_999.0]);
}

/// [삽입 → 저장 → 재파싱] 삽입한 차트가 HWPX 파일로 나가고 다시 읽힌다 — 끝단 계약.
#[test]
fn inserted_chart_survives_save_and_reload() {
    use rhwp::wasm_api::HwpDocument;

    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let spec = r#"{"type":"column","title":"연간 실적",
        "categories":["1월","2월","3월"],
        "series":[{"name":"매출","values":[100,150,130]}]}"#;
    let out = doc.insert_chart(0, 0, spec, 0, 0, false).expect("삽입");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let ci = v["controlIdx"].as_u64().unwrap() as u32;

    // 저장
    let bytes = rhwp::serializer::hwpx::serialize_hwpx(doc.document()).expect("HWPX 저장");

    // 저장된 파일에 차트 파트와 참조가 있어야 한다
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).expect("zip");
        let mut chart_xml = String::new();
        zip.by_name("Chart/chart1.xml")
            .expect("저장본에 Chart/chart1.xml 이 없다")
            .read_to_string(&mut chart_xml)
            .expect("차트 파트 읽기");
        assert!(chart_xml.contains("<c:v>매출</c:v>"), "계열 이름 누락");
        assert!(chart_xml.contains("<a:t>연간 실적</a:t>"), "제목 누락");

        let mut section = String::new();
        zip.by_name("Contents/section0.xml")
            .expect("section0")
            .read_to_string(&mut section)
            .expect("읽기");
        assert!(
            section.contains("chartIDRef=\"Chart/chart1.xml\""),
            "본문 차트 참조 누락"
        );
    }

    // 다시 열어 데이터가 같은지
    let reopened = HwpDocument::from_bytes(&bytes).expect("재파싱");
    let spec_json = reopened
        .get_chart_spec_native(0, 0, ci as usize)
        .expect("재파싱 문서에서 차트 조회");
    let v2: serde_json::Value = serde_json::from_str(&spec_json).unwrap();
    assert_eq!(v2["title"], "연간 실적");
    assert_eq!(v2["categories"][2], "3월");
    assert_eq!(v2["series"][0]["values"][1], 150.0);
}

/// 갤러리에 올리는 **모든 스타일**이 생성 → 파싱 → 렌더까지 통과해야 한다.
/// 통과 못 하는 스타일은 갤러리에서 빼야 한다(사용자가 고르면 빈 차트가 나온다).
#[test]
fn every_gallery_style_builds_parses_renders() {
    use rhwp::ooxml_chart::writer::CHART_STYLES;
    let mut broken: Vec<String> = Vec::new();
    for (id, label) in CHART_STYLES {
        let s = ChartSpec {
            style: (*id).to_string(),
            chart_type: OoxmlChartType::Column,
            title: Some("제목".into()),
            categories: vec!["가".into(), "나".into(), "다".into()],
            series: vec![
                ChartSeriesSpec {
                    name: "A".into(),
                    values: vec![3.0, 5.0, 4.0],
                },
                ChartSeriesSpec {
                    name: "B".into(),
                    values: vec![2.0, 1.0, 6.0],
                },
            ],
        };
        let xml = build_chart_xml(&s);
        let Some(chart) = OoxmlChart::parse(xml.as_bytes()) else {
            broken.push(format!("{id}({label}): 파싱 실패"));
            continue;
        };
        if chart.series.is_empty() {
            broken.push(format!("{id}({label}): 계열 0"));
            continue;
        }
        // 값이 실제로 반영됐는지(템플릿 원본 값이 남으면 데이터 교체 실패)
        let first = &chart.series[0].values;
        if first.is_empty() || (first[0] - 3.0).abs() > 0.001 {
            broken.push(format!("{id}({label}): 값 미반영 {first:?}"));
            continue;
        }
        let svg = chart.render_svg(0.0, 0.0, 400.0, 300.0);
        if svg.len() < 200 {
            broken.push(format!("{id}({label}): SVG {}B", svg.len()));
        }
    }
    assert!(
        broken.is_empty(),
        "갤러리 스타일 결함:\n  {}",
        broken.join("\n  ")
    );
}
