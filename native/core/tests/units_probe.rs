//! A probe for comparing the unit reader with the TS engine line by line (a differential fuzz). It does nothing
//! unless asked: `PROBE_FILE=lines.txt` prints each line's display, quantity and `tryConvert` answer (a line may be
//! `text<TAB>{options json}`, and `a || b` is a sheet); `PROBE_FNS=calls.txt` replays `module<TAB>fn<TAB>[args]`
//! through the units golden dispatch and prints JSON. The TS side prints the same shapes.

#[test]
fn probe_fns() {
    let Ok(path) = std::env::var("PROBE_FNS") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(m), Some(name), Some(args)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let args: Vec<serde_json::Value> = serde_json::from_str(args).unwrap_or_default();
        let out =
            qcalc_core::units::golden::dispatch(m, name, &args).unwrap_or(serde_json::Value::Null);
        println!(
            "{}",
            serde_json::json!({ "mod": m, "name": name, "args": args, "out": out })
        );
    }
}

#[test]
fn probe() {
    let Ok(path) = std::env::var("PROBE_FILE") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let show = |v: Option<qcalc_core::types::Value>| match v {
        None => "None".to_string(),
        Some(v) => {
            let m = v.meas.unwrap_or_default();
            let f = |x: Option<f64>| x.map_or("-".into(), qcalc_core::jsnum::to_string);
            format!(
                "{} {} {} {} {} {} {}",
                qcalc_core::jsnum::to_string(v.n),
                v.unit.unwrap_or_default(),
                v.unit_id.unwrap_or_default(),
                v.text.unwrap_or_default(),
                f(m.sig.map(f64::from)),
                f(m.dp.map(f64::from)),
                f(m.unc)
            )
        }
    };
    for line in text.lines() {
        let (line, opts) = line.split_once('\t').unwrap_or((line, "{}"));
        let o: qcalc_core::types::EvaluateOptions = serde_json::from_str(opts).unwrap_or_default();
        let mut r = qcalc_core::evaluate_line(line, &o);
        if line.contains(" || ") {
            let lines: Vec<String> = line.split(" || ").map(str::to_string).collect();
            let all = qcalc_core::evaluate_sheet(&lines, &o);
            r.display = all
                .iter()
                .map(|x| format!("{}[{}]", x.display, x.quantity.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join(" ; ");
            r.quantity = None;
        }
        let v =
            qcalc_core::units::try_convert_opt(line, &o.default_units, &o.variables, false, None);
        println!(
            "{} => {} q={} | {}",
            serde_json::to_string(line).unwrap_or_default(),
            serde_json::to_string(&r.display).unwrap_or_default(),
            r.quantity.unwrap_or_default(),
            show(v)
        );
    }
}
