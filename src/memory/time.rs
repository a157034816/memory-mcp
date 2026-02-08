use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};

#[derive(Debug, Clone, Copy)]
pub enum DateBoundKind {
    Start,
    End,
}

pub fn now_rfc3339_and_ts() -> (String, i64) {
    let now = Utc::now();
    (
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        now.timestamp(),
    )
}

pub fn now_local_rfc3339_and_offset_seconds() -> (String, i32) {
    let now = Local::now();
    (
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        now.offset().local_minus_utc(),
    )
}

pub fn parse_time_to_ts_and_canonical(
    input: &str,
    bound: DateBoundKind,
) -> Result<(i64, String), String> {
    let text = input.trim();
    if text.is_empty() {
        return Err("时间不能为空".to_string());
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        let utc = dt.with_timezone(&Utc);
        return Ok((
            utc.timestamp(),
            utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ));
    }

    // 容错：部分调用方会将 RFC3339 中的 'T'/'Z' 变成小写（例如经过 to_lowercase）。
    // 这里做一次最小修补后再尝试解析，以提升 remember/recall 的对接鲁棒性。
    if let Some(patched) = patch_rfc3339_case(text) {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&patched) {
            let utc = dt.with_timezone(&Utc);
            return Ok((
                utc.timestamp(),
                utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ));
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let dt = match bound {
            DateBoundKind::Start => Utc.from_utc_datetime(
                &date
                    .and_hms_opt(0, 0, 0)
                    .ok_or_else(|| "无效日期".to_string())?,
            ),
            DateBoundKind::End => Utc.from_utc_datetime(
                &date
                    .and_hms_opt(23, 59, 59)
                    .ok_or_else(|| "无效日期".to_string())?,
            ),
        };
        return Ok((dt.timestamp(), date.format("%Y-%m-%d").to_string()));
    }

    Err("时间格式不支持：仅支持 RFC3339 或 YYYY-MM-DD".to_string())
}

fn patch_rfc3339_case(text: &str) -> Option<String> {
    if !text.is_ascii() {
        return None;
    }

    let bytes = text.as_bytes();
    let mut buf = bytes.to_vec();
    let mut changed = false;

    // RFC3339：YYYY-MM-DD'T'...（允许输入被 lowercased 成 't'）
    if buf.len() > 10 && buf[10] == b't' {
        buf[10] = b'T';
        changed = true;
    }

    // 结尾 'Z' 可能被 lowercased 成 'z'
    if let Some(last) = buf.last_mut() {
        if *last == b'z' {
            *last = b'Z';
            changed = true;
        }
    }

    if !changed {
        return None;
    }

    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_should_accept_lowercase_rfc3339_t_z() {
        let (ts1, c1) =
            parse_time_to_ts_and_canonical("2025-08-20T10:00:00Z", DateBoundKind::Start)
                .expect("parse upper");
        let (ts2, c2) =
            parse_time_to_ts_and_canonical("2025-08-20t10:00:00z", DateBoundKind::Start)
                .expect("parse lower");
        assert_eq!(ts1, ts2);
        assert_eq!(c1, c2);
    }
}
