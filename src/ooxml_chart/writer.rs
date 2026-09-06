//! OOXML 차트 XML **생성기** — 데이터 → `c:chartSpace` XML.
//!
//! ## 설계: 백지 생성이 아니라 템플릿 패치
//!
//! 한컴이 저장한 실물 차트 XML(`templates/*.xml`)을 그대로 두고 **데이터가 들어가는
//! 캐시(`c:tx`/`c:cat`/`c:val`)와 제목만 갈아끼운다**. 이유 셋:
//!
//! 1. 한컴 고유 확장(`c:extLst` 의 `ho:hncChartStyle`, 함초롬돋움 `c:txPr`, 축·눈금·
//!    범례 서식)이 자동으로 보존된다 — 백지 생성이면 이것들을 하나씩 재현해야 하고,
//!    빠뜨리면 한컴에서 밋밋하거나 깨져 보인다.
//! 2. **생성과 편집이 같은 함수**가 된다. 새 차트는 우리 템플릿을 패치하고, 기존 차트
//!    편집은 그 문서의 XML 을 패치한다 — 편집이 사용자 서식을 지우지 않는다.
//! 3. 계열별 부속 요소(막대의 `c:invertIfNegative`, 원형의 `c:explosion`, 꺾은선의
//!    `c:marker`·`c:smooth`)가 종류마다 다른데, 템플릿의 계열 블록을 **복제**하면
//!    종류별 분기 없이 맞는다.
//!
//! 파서(`super::parser`)와 짝이며, `parse(patch(xml, spec)) == spec` 왕복이 계약이다.

use super::OoxmlChartType;

/// 차트 한 계열 — 이름과 값.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartSeriesSpec {
    pub name: String,
    pub values: Vec<f64>,
}

/// 차트 생성·편집 입력 — 스튜디오 대화상자가 채우는 값 그대로.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartSpec {
    /// 갤러리에서 고른 스타일 id(예: "column-stacked"). 비면 chart_type 에서 유도.
    pub style: String,
    pub chart_type: OoxmlChartType,
    pub title: Option<String>,
    /// 항목(가로축) 라벨
    pub categories: Vec<String>,
    pub series: Vec<ChartSeriesSpec>,
}

const TPL_COLUMN: &str = include_str!("templates/column.xml");
const TPL_COLUMN_STACKED: &str = include_str!("templates/column-stacked.xml");
const TPL_COLUMN_100: &str = include_str!("templates/column-100.xml");
const TPL_COLUMN_3D: &str = include_str!("templates/column-3d.xml");
const TPL_COLUMN_3D_STACKED: &str = include_str!("templates/column-3d-stacked.xml");
const TPL_BAR: &str = include_str!("templates/bar.xml");
const TPL_BAR_STACKED: &str = include_str!("templates/bar-stacked.xml");
const TPL_BAR_100: &str = include_str!("templates/bar-100.xml");
const TPL_BAR_3D: &str = include_str!("templates/bar-3d.xml");
const TPL_BAR_3D_STACKED: &str = include_str!("templates/bar-3d-stacked.xml");
const TPL_LINE: &str = include_str!("templates/line.xml");
const TPL_LINE_MARKER: &str = include_str!("templates/line-marker.xml");
const TPL_LINE_STACKED: &str = include_str!("templates/line-stacked.xml");
const TPL_LINE_100: &str = include_str!("templates/line-100.xml");
const TPL_LINE_MARKER_STACKED: &str = include_str!("templates/line-marker-stacked.xml");
const TPL_PIE: &str = include_str!("templates/pie.xml");
const TPL_PIE_3D: &str = include_str!("templates/pie-3d.xml");
const TPL_PIE_EXPLODED: &str = include_str!("templates/pie-exploded.xml");
const TPL_PIE_OF_PIE: &str = include_str!("templates/pie-of-pie.xml");
const TPL_PIE_OF_BAR: &str = include_str!("templates/pie-of-bar.xml");
const TPL_SCATTER: &str = include_str!("templates/scatter.xml");
const TPL_SCATTER_LINE: &str = include_str!("templates/scatter-line.xml");
const TPL_SCATTER_SMOOTH: &str = include_str!("templates/scatter-smooth.xml");

/// 스타일 id → 한컴 실물 템플릿. 알 수 없는 id 는 묶은 세로 막대형.
pub fn template_for_style(style: &str) -> &'static str {
    match style {
        "column" => TPL_COLUMN,
        "column-stacked" => TPL_COLUMN_STACKED,
        "column-100" => TPL_COLUMN_100,
        "column-3d" => TPL_COLUMN_3D,
        "column-3d-stacked" => TPL_COLUMN_3D_STACKED,
        "bar" => TPL_BAR,
        "bar-stacked" => TPL_BAR_STACKED,
        "bar-100" => TPL_BAR_100,
        "bar-3d" => TPL_BAR_3D,
        "bar-3d-stacked" => TPL_BAR_3D_STACKED,
        "line" => TPL_LINE,
        "line-marker" => TPL_LINE_MARKER,
        "line-stacked" => TPL_LINE_STACKED,
        "line-100" => TPL_LINE_100,
        "line-marker-stacked" => TPL_LINE_MARKER_STACKED,
        "pie" => TPL_PIE,
        "pie-3d" => TPL_PIE_3D,
        "pie-exploded" => TPL_PIE_EXPLODED,
        "pie-of-pie" => TPL_PIE_OF_PIE,
        "pie-of-bar" => TPL_PIE_OF_BAR,
        "scatter" => TPL_SCATTER,
        "scatter-line" => TPL_SCATTER_LINE,
        "scatter-smooth" => TPL_SCATTER_SMOOTH,
        _ => TPL_COLUMN,
    }
}

/// 갤러리에 보여줄 (스타일 id, 한글 이름) 목록 — 스튜디오가 이걸로 팔레트를 만든다.
pub const CHART_STYLES: &[(&str, &str)] = &[
    ("column", "묶은 세로 막대형"),
    ("column-stacked", "누적 세로 막대형"),
    ("column-100", "100% 기준 누적 세로 막대형"),
    ("column-3d", "3차원 묶은 세로 막대형"),
    ("column-3d-stacked", "3차원 누적 세로 막대형"),
    ("bar", "묶은 가로 막대형"),
    ("bar-stacked", "누적 가로 막대형"),
    ("bar-100", "100% 기준 누적 가로 막대형"),
    ("bar-3d", "3차원 묶은 가로 막대형"),
    ("bar-3d-stacked", "3차원 누적 가로 막대형"),
    ("line", "꺾은선형"),
    ("line-marker", "표식이 있는 꺾은선형"),
    ("line-stacked", "누적 꺾은선형"),
    ("line-100", "100% 기준 누적 꺾은선형"),
    ("line-marker-stacked", "표식이 있는 누적 꺾은선형"),
    ("pie", "원형"),
    ("pie-3d", "3차원 원형"),
    ("pie-exploded", "쪼개진 원형"),
    ("pie-of-pie", "원형 대 원형"),
    ("pie-of-bar", "원형 대 가로 막대형"),
    ("scatter", "표식만 있는 분산형"),
    ("scatter-line", "직선이 있는 분산형"),
    ("scatter-smooth", "곡선이 있는 분산형"),
];

/// 새 차트 XML 을 만든다 — 종류에 맞는 한컴 템플릿을 spec 으로 패치.
pub fn build_chart_xml(spec: &ChartSpec) -> String {
    let tpl = if spec.style.is_empty() {
        template_for_style(default_style_of(spec.chart_type))
    } else {
        template_for_style(&spec.style)
    };
    patch_chart_xml(tpl, spec)
}

/// 종류만 주어졌을 때의 기본 스타일 id(구 API 호환).
pub fn default_style_of(kind: OoxmlChartType) -> &'static str {
    match kind {
        OoxmlChartType::Bar => "bar",
        OoxmlChartType::Line => "line",
        OoxmlChartType::Pie => "pie",
        OoxmlChartType::Scatter => "scatter",
        _ => "column",
    }
}

/// 템플릿(또는 기존 차트)의 **플롯 서명** — 플롯 요소 + 막대 방향 + 그룹핑.
/// 스타일이 바뀌었는지(패치로 될지, 새로 만들어야 할지) 가르는 단일 판정.
pub fn plot_signature(xml: &str) -> String {
    let plot = [
        "bar3DChart",
        "barChart",
        "line3DChart",
        "lineChart",
        "pie3DChart",
        "ofPieChart",
        "doughnutChart",
        "pieChart",
        "areaChart",
        "scatterChart",
        "stockChart",
        "radarChart",
    ]
    .iter()
    .find(|t| xml.contains(&format!("<c:{t}>")))
    .copied()
    .unwrap_or("unknown");
    let attr = |tag: &str| -> String {
        let pat = format!("<c:{tag} val=\"");
        match xml.find(&pat) {
            Some(i) => {
                let vs = i + pat.len();
                xml[vs..]
                    .find('"')
                    .map(|r| xml[vs..vs + r].to_string())
                    .unwrap_or_default()
            }
            None => String::new(),
        }
    };
    format!("{plot}/{}/{}", attr("barDir"), attr("grouping"))
}

/// 기존(또는 템플릿) 차트 XML 의 **데이터와 제목만** 교체한다. 나머지 서식은 보존.
pub fn patch_chart_xml(xml: &str, spec: &ChartSpec) -> String {
    let out = patch_series(xml, spec);
    patch_title(&out, spec.title.as_deref())
}

// ── XML 조각 도구 ───────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// 열 번호(0-based) → 엑셀 열 문자(A, B, … Z, AA…). 계열 참조 `Sheet1!$B$1` 용.
fn col_letter(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

/// `<tag>`…`</tag>` 중 **첫 블록**의 시작·끝 바이트 범위(태그 포함).
fn block_range(xml: &str, tag: &str) -> Option<(usize, usize)> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = xml.find(&open)?;
    let e = xml[s..].find(&close)? + s + close.len();
    Some((s, e))
}

/// 블록의 **내용만** 교체(태그는 유지).
fn replace_block_inner(xml: &str, tag: &str, new_inner: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    match block_range(xml, tag) {
        Some((s, e)) => {
            let mut out = String::with_capacity(xml.len() + new_inner.len());
            out.push_str(&xml[..s]);
            out.push_str(&open);
            out.push_str(new_inner);
            out.push_str(&close);
            out.push_str(&xml[e..]);
            out
        }
        None => xml.to_string(),
    }
}

/// `<tag val="…"/>` 의 값을 바꾼다(첫 등장).
fn set_val_attr(xml: &str, tag: &str, val: &str) -> String {
    let pat = format!("<{tag} val=\"");
    match xml.find(&pat) {
        Some(s) => {
            let vs = s + pat.len();
            match xml[vs..].find('"') {
                Some(rel) => format!("{}{}{}", &xml[..vs], val, &xml[vs + rel..]),
                None => xml.to_string(),
            }
        }
        None => xml.to_string(),
    }
}

fn str_cache(items: &[String]) -> String {
    let mut s = format!("<c:ptCount val=\"{}\"/>", items.len());
    for (i, v) in items.iter().enumerate() {
        s.push_str(&format!(
            "<c:pt idx=\"{}\"><c:v>{}</c:v></c:pt>",
            i,
            xml_escape(v)
        ));
    }
    s
}

fn num_cache(values: &[f64]) -> String {
    let mut s = String::from("<c:formatCode>General</c:formatCode>");
    s.push_str(&format!("<c:ptCount val=\"{}\"/>", values.len()));
    for (i, v) in values.iter().enumerate() {
        // 정수는 정수로(한컴 실물이 그렇다), 소수는 불필요한 0 없이
        let text = if v.fract() == 0.0 && v.abs() < 1e15 {
            format!("{}", *v as i64)
        } else {
            let t = format!("{v}");
            t
        };
        s.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{text}</c:v></c:pt>"));
    }
    s
}

// ── 계열 패치 ───────────────────────────────────────────────────────

/// 템플릿의 계열 블록을 복제해 spec 의 계열 수만큼 만든다.
/// 종류별 부속 요소(invertIfNegative/explosion/marker…)가 자동으로 따라온다.
fn patch_series(xml: &str, spec: &ChartSpec) -> String {
    // 계열 블록 전체 범위(첫 <c:ser> ~ 마지막 </c:ser>)
    let Some(first) = xml.find("<c:ser>") else {
        return xml.to_string();
    };
    let Some(last_rel) = xml.rfind("</c:ser>") else {
        return xml.to_string();
    };
    let last = last_rel + "</c:ser>".len();

    // 템플릿 계열들을 모아 둔다 — i 번째가 없으면 마지막 것을 재사용(색 순환은 한컴이 처리)
    let mut templates: Vec<&str> = Vec::new();
    let mut cur = first;
    while let Some(rel) = xml[cur..last].find("<c:ser>") {
        let s = cur + rel;
        let Some(erel) = xml[s..last].find("</c:ser>") else {
            break;
        };
        let e = s + erel + "</c:ser>".len();
        templates.push(&xml[s..e]);
        cur = e;
    }
    if templates.is_empty() {
        return xml.to_string();
    }

    // 원형 차트는 계열이 하나뿐이다(한컴·엑셀 공통) — 첫 계열만 쓴다.
    // 원형 계열(pie/도넛)은 계열이 하나뿐이다 — 스타일 id 우선, 없으면 종류로.
    let is_pie = if spec.style.is_empty() {
        matches!(spec.chart_type, OoxmlChartType::Pie)
    } else {
        spec.style.starts_with("pie") || spec.style.starts_with("doughnut")
    };
    let use_series: Vec<&ChartSeriesSpec> = if is_pie {
        spec.series.iter().take(1).collect()
    } else {
        spec.series.iter().collect()
    };
    if use_series.is_empty() {
        return xml.to_string();
    }

    let cat_ref = format!("Sheet1!$A$2:$A${}", spec.categories.len() + 1);
    let cats = str_cache(&spec.categories);

    let mut built = String::new();
    for (i, ser) in use_series.iter().enumerate() {
        let tpl = templates
            .get(i)
            .copied()
            .unwrap_or_else(|| templates[templates.len() - 1]);
        let mut s = tpl.to_string();
        s = set_val_attr(&s, "c:idx", &i.to_string());
        s = set_val_attr(&s, "c:order", &i.to_string());

        let letter = col_letter(i + 1); // A=항목, B부터 계열
                                        // 계열 이름
        let tx_inner = format!(
            "<c:strRef><c:f>Sheet1!${letter}$1</c:f><c:strCache>{}</c:strCache></c:strRef>",
            str_cache(std::slice::from_ref(&ser.name))
        );
        s = replace_block_inner(&s, "c:tx", &tx_inner);
        if s.contains("<c:xVal>") {
            // ── 분산형(x,y 산점도) ── 항목/값이 아니라 **숫자쌍**이다. 항목 라벨이 숫자면
            // 그대로 X 로 쓰고(예: "0.7"), 아니면 1,2,3… 을 X 로 둔다.
            let xs: Vec<f64> = spec
                .categories
                .iter()
                .enumerate()
                .map(|(k, c)| c.trim().parse::<f64>().unwrap_or((k + 1) as f64))
                .collect();
            let x_inner = format!(
                "<c:numRef><c:f>Sheet1!$A$2:$A${}</c:f><c:numCache>{}</c:numCache></c:numRef>",
                xs.len() + 1,
                num_cache(&xs)
            );
            s = replace_block_inner(&s, "c:xVal", &x_inner);
            let y_inner = format!(
                "<c:numRef><c:f>Sheet1!${letter}$2:${letter}${}</c:f><c:numCache>{}</c:numCache></c:numRef>",
                ser.values.len() + 1,
                num_cache(&ser.values)
            );
            s = replace_block_inner(&s, "c:yVal", &y_inner);
        } else {
            // 항목
            let cat_inner =
                format!("<c:strRef><c:f>{cat_ref}</c:f><c:strCache>{cats}</c:strCache></c:strRef>");
            s = replace_block_inner(&s, "c:cat", &cat_inner);
            // 값
            let val_inner = format!(
                "<c:numRef><c:f>Sheet1!${letter}$2:${letter}${}</c:f><c:numCache>{}</c:numCache></c:numRef>",
                ser.values.len() + 1,
                num_cache(&ser.values)
            );
            s = replace_block_inner(&s, "c:val", &val_inner);
        }
        built.push_str(&s);
    }

    let mut out = String::with_capacity(xml.len() + built.len());
    out.push_str(&xml[..first]);
    out.push_str(&built);
    out.push_str(&xml[last..]);
    out
}

// ── 제목 패치 ───────────────────────────────────────────────────────

/// 제목 텍스트를 넣거나(Some) 지운다(None). 한컴 템플릿의 `c:title` 은 서식만 있고
/// 글자가 없으므로, 글자를 넣을 때 `c:tx/c:rich` 를 만들어 붙인다.
fn patch_title(xml: &str, title: Option<&str>) -> String {
    let Some((s, e)) = block_range(xml, "c:title") else {
        return xml.to_string();
    };
    let inner = &xml[s + "<c:title>".len()..e - "</c:title>".len()];

    let new_inner = match title {
        Some(t) if !t.is_empty() => {
            // 기존 c:tx 가 있으면 통째 교체, 없으면 맨 앞에 삽입(스키마 순서: tx → layout → overlay)
            let rich = format!(
                "<c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx>",
                xml_escape(t)
            );
            match block_range(inner, "c:tx") {
                Some((ts, te)) => format!("{}{}{}", &inner[..ts], rich, &inner[te..]),
                None => format!("{rich}{inner}"),
            }
        }
        _ => match block_range(inner, "c:tx") {
            Some((ts, te)) => format!("{}{}", &inner[..ts], &inner[te..]),
            None => inner.to_string(),
        },
    };

    let mut out = String::with_capacity(xml.len() + new_inner.len());
    out.push_str(&xml[..s]);
    out.push_str("<c:title>");
    out.push_str(&new_inner);
    out.push_str("</c:title>");
    out.push_str(&xml[e..]);
    // 제목을 넣으면 autoTitleDeleted 는 0 이어야 표시된다
    if title.is_some_and(|t| !t.is_empty()) {
        out = set_val_attr(&out, "c:autoTitleDeleted", "0");
    }
    out
}
