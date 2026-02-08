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
