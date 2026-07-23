use rhwp::DocumentCore;
use serde_json::Value;

fn made_ids(made: &str) -> (usize, usize) {
    let v: Value = serde_json::from_str(made).unwrap();
    (
        v["paraIdx"].as_u64().unwrap() as usize,
        v["controlIdx"].as_u64().unwrap() as usize,
    )
}

#[test]
fn probe_fit_after_margin() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let (para, ctrl) = made_ids(
        &core
            .create_table_ex_native(0, 0, 0, 2, 3, false, None, None)
            .unwrap(),
    );
    // 넘침 신호(읽기전용)가 처음엔 fits=true
    let f0: Value = serde_json::from_str(&core.get_table_fit_native(0, para, ctrl).unwrap()).unwrap();
    eprintln!("fit query initial: {}", core.get_table_fit_native(0, para, ctrl).unwrap());
    assert_eq!(f0["fits"], Value::Bool(true));

    core.set_page_def_native(0, r#"{"marginLeft":20000,"marginRight":20000}"#)
        .unwrap();
    // 여백 확대 후 넘침 신호가 켜져야 한다
    let f1: Value = serde_json::from_str(&core.get_table_fit_native(0, para, ctrl).unwrap()).unwrap();
    eprintln!("fit query after margin: {}", core.get_table_fit_native(0, para, ctrl).unwrap());
    assert_eq!(f1["fits"], Value::Bool(false), "margin change should overflow");
    assert!(f1["overflow"].as_u64().unwrap() > 0);

    let fit = core.fit_table_to_page_native(0, para, ctrl).unwrap();
    eprintln!("margin fit: {}", fit);
    let v: Value = serde_json::from_str(&fit).unwrap();
    assert_eq!(v["changed"], Value::Bool(true), "fit should trigger");
    assert!(v["tableWidth"].as_u64().unwrap() <= v["pageContentWidth"].as_u64().unwrap());

    // 보정 후 넘침 신호는 다시 꺼져야 한다
    let f2: Value = serde_json::from_str(&core.get_table_fit_native(0, para, ctrl).unwrap()).unwrap();
    assert_eq!(f2["fits"], Value::Bool(true), "after fit should not overflow");
}

#[test]
fn probe_fit_after_paper_shrink() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let (para, ctrl) = made_ids(
        &core
            .create_table_ex_native(0, 0, 0, 2, 3, false, None, None)
            .unwrap(),
    );
    core.set_page_def_native(0, r#"{"width":36000,"height":51000}"#)
        .unwrap();
    let f1: Value = serde_json::from_str(&core.get_table_fit_native(0, para, ctrl).unwrap()).unwrap();
    assert_eq!(f1["fits"], Value::Bool(false));
    core.fit_table_to_page_native(0, para, ctrl).unwrap();
    let f2: Value = serde_json::from_str(&core.get_table_fit_native(0, para, ctrl).unwrap()).unwrap();
    eprintln!("A6 after fit query: {}", core.get_table_fit_native(0, para, ctrl).unwrap());
    assert_eq!(f2["fits"], Value::Bool(true));
}

#[test]
fn probe_set_absolute_widths() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let (para, ctrl) = made_ids(
        &core
            .create_table_ex_native(0, 0, 0, 2, 3, false, None, None)
            .unwrap(),
    );
    let r = core
        .set_table_column_widths_native(0, para, ctrl, vec![4000, 4000, 4000])
        .unwrap();
    eprintln!("set widths: {}", r);
    let v: Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["tableWidth"].as_u64().unwrap(), 12000);
    // 열 수 불일치는 거부해야 한다
    assert!(core
        .set_table_column_widths_native(0, para, ctrl, vec![4000, 4000])
        .is_err());
}
