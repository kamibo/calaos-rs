use std::error::Error;

use crate::config;
use crate::io_context;

use io_context::BroadcastIODataActionTx;
use io_context::InputContextMap;

use chrono::prelude::*;
use chrono::DateTime;
use chrono::Local;
use std::time::Duration;

use config::io::InputKind;

pub async fn run(
    _tx_command: BroadcastIODataActionTx,
    /* mut */ input_map: InputContextMap<'_>,
) -> Result<(), Box<dyn Error>> {
    loop {
        let now = Local::now();
        let all_durations: Vec<_> = input_map
            .iter()
            .map(|(k, v)| (*k, unwrap_date(&v.input.kind)))
            .collect();
        let ordered_durations = to_ordered_durations(now, all_durations);
        if ordered_durations.is_empty() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        } else {
            let next = ordered_durations[0];
            tokio::time::sleep(next.1).await;
        }
    }
}

fn unwrap_date(kind: &InputKind) -> config::io::Date {
    match kind {
        InputKind::InputTime(date) => date.clone(),
        _ => panic!("Expected input time"),
    }
}

fn to_ordered_durations(
    now: DateTime<Local>,
    dates: Vec<(&str, config::io::Date)>,
) -> Vec<(&str, Duration)> {
    let mut res: Vec<_> = dates
        .iter()
        .map(|(k, d)| (k, to_duration(now, d)))
        .filter(|(_, d)| d.is_some())
        .map(|(k, d)| (*k, d.unwrap()))
        .collect();

    res.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    res
}

fn to_duration(now: DateTime<Local>, date: &config::io::Date) -> Option<Duration> {
    let datetime = Local
        .ymd(
            date.year.unwrap_or_else(|| now.year()),
            date.month.unwrap_or_else(|| now.month()),
            date.day.unwrap_or_else(|| now.day()),
        )
        .and_hms(date.hour, date.min, date.sec);

    if now > datetime {
        return None;
    }

    Some((datetime - now).to_std().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::io::Date;

    #[test]
    fn date_to_durations() {
        let now = Local.ymd(2020, 1, 1).and_hms(0, 0, 0);
        let d1 = Date {
            year: Some(2020),
            month: Some(1),
            day: Some(1),
            hour: 1,
            min: 0,
            sec: 0,
        };
        let d2 = Date {
            year: Some(2020),
            month: Some(1),
            day: Some(1),
            hour: 0,
            min: 1,
            sec: 0,
        };
        let d3 = Date {
            year: Some(2019),
            month: Some(1),
            day: Some(1),
            hour: 0,
            min: 1,
            sec: 0,
        };
        // d3 should be ignored as < now
        let d4 = Date {
            year: None,
            month: None,
            day: None,
            hour: 0,
            min: 1,
            sec: 1,
        };
        let durations =
            to_ordered_durations(now, vec![("d1", d1), ("d2", d2), ("d3", d3), ("d4", d4)]);
        assert!(
            durations
                == vec![
                    ("d2", Duration::from_secs(60)),
                    ("d4", Duration::from_secs(61)),
                    ("d1", Duration::from_secs(3600))
                ]
        );
    }
}
