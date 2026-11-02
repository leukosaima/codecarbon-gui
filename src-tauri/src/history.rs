//! Past sessions, read from CodeCarbon's `emissions.csv` (History view).
//!
//! CodeCarbon appends one row per measurement session when it stops. The
//! file is CodeCarbon's own format, whose columns change between versions,
//! so it is parsed **by header name**: unknown columns are ignored, missing
//! optional columns are `None`, and rows that cannot be read are skipped
//! and counted (never fatal).
//!
//! Everything here is pure (no Tauri), so it is unit-tested below. Rust
//! returns raw numbers; the panel formats them (`src/format.ts`).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, TimeDelta};
use serde::{Deserialize, Serialize, Serializer};

/// File name CodeCarbon writes in the data folder.
pub const CSV_FILE: &str = "emissions.csv";

/// Length of the daily series returned by [`summarise`], today included.
pub const DAILY_DAYS: u64 = 30;

/// Format of `started_at` / CodeCarbon's `timestamp` (local time, no offset).
const ISO_LOCAL: &str = "%Y-%m-%dT%H:%M:%S";

/// One CSV row as CodeCarbon writes it. Only the columns we use; serde
/// matches them by header name. Numbers go through `invalid_option`, so a
/// garbled value becomes `None` instead of failing the whole file.
#[derive(Debug, Deserialize)]
struct RawRow {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    /// Seconds.
    #[serde(default, deserialize_with = "csv::invalid_option")]
    duration: Option<f64>,
    /// kg CO₂e.
    #[serde(default, deserialize_with = "csv::invalid_option")]
    emissions: Option<f64>,
    /// kWh.
    #[serde(default, deserialize_with = "csv::invalid_option")]
    energy_consumed: Option<f64>,
    /// Watts (fallback for the average power of a zero-length session).
    #[serde(default, deserialize_with = "csv::invalid_option")]
    cpu_power: Option<f64>,
    #[serde(default, deserialize_with = "csv::invalid_option")]
    gpu_power: Option<f64>,
    #[serde(default, deserialize_with = "csv::invalid_option")]
    ram_power: Option<f64>,
    #[serde(default)]
    country_name: Option<String>,
}

/// A past measurement session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Session {
    /// Local start time, `YYYY-MM-DDTHH:MM:SS`. CodeCarbon's `timestamp`
    /// is taken when the run stops, so this is `timestamp − duration`.
    #[serde(serialize_with = "iso_local")]
    pub started_at: NaiveDateTime,
    pub duration_s: f64,
    pub energy_kwh: f64,
    pub emissions_kg: f64,
    /// Energy / duration.
    pub avg_power_w: f64,
    pub country_name: Option<String>,
    pub project_name: String,
    pub run_id: String,
}

fn iso_local<S: Serializer>(t: &NaiveDateTime, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&t.format(ISO_LOCAL))
}

/// Everything read from `emissions.csv`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct History {
    /// Newest first.
    pub sessions: Vec<Session>,
    /// Rows that could not be read (malformed, or missing a required value).
    pub skipped_rows: usize,
    pub path: String,
}

/// `2026-11-02T22:53:44`, tolerating fractional seconds and a space.
fn parse_timestamp(text: &str) -> Option<NaiveDateTime> {
    let text = text.trim();
    ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(text, f).ok())
}

fn finite_non_negative(x: Option<f64>) -> Option<f64> {
    x.filter(|v| v.is_finite() && *v >= 0.0)
}

impl RawRow {
    /// `None` when a required value (time, duration, energy, CO₂e) is
    /// missing or not a number.
    fn into_session(self) -> Option<Session> {
        let ended_at = parse_timestamp(self.timestamp.as_deref()?)?;
        let duration_s = finite_non_negative(self.duration)?;
        let energy_kwh = finite_non_negative(self.energy_consumed)?;
        let emissions_kg = finite_non_negative(self.emissions)?;
        let started_at = TimeDelta::try_seconds(duration_s.round() as i64)
            .and_then(|d| ended_at.checked_sub_signed(d))
            .unwrap_or(ended_at);
        let avg_power_w = if duration_s > 0.0 {
            energy_kwh * 3_600_000.0 / duration_s
        } else {
            [self.cpu_power, self.gpu_power, self.ram_power]
                .into_iter()
                .filter_map(finite_non_negative)
                .sum()
        };
        let text = |s: Option<String>| s.map(|s| s.trim().to_string()).unwrap_or_default();
        Some(Session {
            started_at,
            duration_s,
            energy_kwh,
            emissions_kg,
            avg_power_w,
            country_name: self
                .country_name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            project_name: text(self.project_name),
            run_id: text(self.run_id),
        })
    }
}

/// Parse CSV text. Returns the sessions (newest first) and the number of
/// rows that were skipped.
pub fn parse<R: Read>(reader: R) -> anyhow::Result<(Vec<Session>, usize)> {
    let mut csv = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);
    // An unreadable header (e.g. not UTF-8) is an error for the whole file.
    csv.headers()?;
    let mut sessions = Vec::new();
    let mut skipped = 0;
    for row in csv.deserialize::<RawRow>() {
        match row.ok().and_then(RawRow::into_session) {
            Some(session) => sessions.push(session),
            None => skipped += 1,
        }
    }
    sessions.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });
    Ok((sessions, skipped))
}

/// Read `emissions.csv`. A missing file is an empty history, not an error.
pub fn load(path: &Path) -> anyhow::Result<History> {
    let display = path.to_string_lossy().into_owned();
    let (sessions, skipped_rows) = match File::open(path) {
        Ok(file) => parse(file)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), 0),
        Err(e) => return Err(e.into()),
    };
    Ok(History {
        sessions,
        skipped_rows,
        path: display,
    })
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Sum over a set of sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Totals {
    pub sessions: usize,
    pub duration_s: f64,
    pub energy_kwh: f64,
    pub emissions_kg: f64,
}

impl Totals {
    fn add(&mut self, s: &Session) {
        self.sessions += 1;
        self.duration_s += s.duration_s;
        self.energy_kwh += s.energy_kwh;
        self.emissions_kg += s.emissions_kg;
    }
}

/// One day of the daily series.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayTotal {
    /// `YYYY-MM-DD` (local).
    pub date: String,
    pub energy_kwh: f64,
    pub emissions_kg: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub today: Totals,
    /// ISO week containing today (from Monday).
    pub this_week: Totals,
    pub this_month: Totals,
    pub all_time: Totals,
    /// The last [`DAILY_DAYS`] days, oldest first, today last, zero-filled.
    pub daily: Vec<DayTotal>,
}

/// Totals for today / this week / this month (so far) / all time, and a
/// daily series. A session counts for the day it started on; one dated
/// after today (clock changes) only counts for all time.
pub fn summarise(sessions: &[Session], now_local: NaiveDateTime) -> Summary {
    let today = now_local.date();
    let week_start = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let first_day = today - Days::new(DAILY_DAYS - 1);

    let mut daily: Vec<(NaiveDate, Totals)> = (0..DAILY_DAYS)
        .map(|i| (first_day + Days::new(i), Totals::default()))
        .collect();
    let mut summary = Summary {
        today: Totals::default(),
        this_week: Totals::default(),
        this_month: Totals::default(),
        all_time: Totals::default(),
        daily: Vec::new(),
    };
    for s in sessions {
        let day = s.started_at.date();
        summary.all_time.add(s);
        if day == today {
            summary.today.add(s);
        }
        if (week_start..=today).contains(&day) {
            summary.this_week.add(s);
        }
        if day <= today && day.year() == today.year() && day.month() == today.month() {
            summary.this_month.add(s);
        }
        if (first_day..=today).contains(&day) {
            let index = (day - first_day).num_days() as usize;
            daily[index].1.add(s);
        }
    }
    summary.daily = daily
        .into_iter()
        .map(|(date, t)| DayTotal {
            date: date.format("%Y-%m-%d").to_string(),
            energy_kwh: t.energy_kwh,
            emissions_kg: t.emissions_kg,
            duration_s: t.duration_s,
        })
        .collect();
    summary
}

// ---------------------------------------------------------------------------
// Everyday equivalents
// ---------------------------------------------------------------------------

/// What a factor converts: the CO₂e of a session, or its energy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// `per_unit` is kg CO₂e per unit.
    Emissions,
    /// `per_unit` is kWh per unit.
    Energy,
}

/// One "everyday equivalent" conversion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Factor {
    pub key: &'static str,
    pub unit: &'static str,
    pub basis: Basis,
    pub per_unit: f64,
}

/// Conversion factors, deliberately round and conservative (they are
/// orders of magnitude, not precise accounting). Shown in this order; the
/// dashboard picks its one-liner from this list (see `src/equivalents.ts`).
pub const FACTORS: [Factor; 4] = [
    // ≈ 0.17 kg CO₂e per km for an average passenger car (unknown fuel,
    // tailpipe + fuel production), the figure used for European fleets.
    // UK DESNZ / DEFRA "Greenhouse gas reporting: conversion factors 2023",
    // Passenger vehicles → Cars (by size) → Average car, unknown fuel:
    // https://www.gov.uk/government/publications/greenhouse-gas-reporting-conversion-factors-2023
    // (EEA context: https://www.eea.europa.eu/en/analysis/indicators/co2-performance-of-new-passenger)
    Factor {
        key: "car_km",
        unit: "km",
        basis: Basis::Emissions,
        per_unit: 0.17,
    },
    // ≈ 0.008 kg CO₂ per full smartphone charge, grid losses included
    // (≈ 8.22 × 10⁻⁶ t CO₂ per smartphone charged in the EPA calculator).
    // US EPA, Greenhouse Gas Equivalencies Calculator, "Number of
    // smartphones charged":
    // https://www.epa.gov/energy/greenhouse-gas-equivalencies-calculator-calculations-and-references
    Factor {
        key: "smartphone_charges",
        unit: "charges",
        basis: Basis::Emissions,
        per_unit: 0.008,
    },
    // ≈ 0.1 kWh per hour for a typical LED TV (~100 W while on). Uses the
    // session's *energy*, so it does not depend on the grid.
    // US DOE Energy Saver, "Estimating Appliance and Home Electronic Energy
    // Use" (typical wattages):
    // https://www.energy.gov/energysaver/estimating-appliance-and-home-electronic-energy-use
    Factor {
        key: "led_tv_hours",
        unit: "hours",
        basis: Basis::Energy,
        per_unit: 0.1,
    },
    // ≈ 0.055 kg CO₂e per hour of video streaming (data centres, networks
    // and device), IEA 2020 best estimate (G. Kamiya, "The carbon footprint
    // of streaming video: fact-checking the headlines"):
    // https://www.iea.org/commentaries/the-carbon-footprint-of-streaming-video-fact-checking-the-headlines
    Factor {
        key: "streaming_hours",
        unit: "hours",
        basis: Basis::Emissions,
        per_unit: 0.055,
    },
];

/// `value` `unit` of `key`, e.g. 12.3 km by car (`car_km`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Equivalent {
    pub key: &'static str,
    pub value: f64,
    pub unit: &'static str,
}

/// The everyday equivalents of a session (or a total), in [`FACTORS`]
/// order. Non-finite or negative inputs count as 0.
pub fn equivalents(emissions_kg: f64, energy_kwh: f64) -> Vec<Equivalent> {
    let clean = |x: f64| if x.is_finite() && x > 0.0 { x } else { 0.0 };
    let (emissions_kg, energy_kwh) = (clean(emissions_kg), clean(energy_kwh));
    FACTORS
        .iter()
        .map(|f| Equivalent {
            key: f.key,
            unit: f.unit,
            value: match f.basis {
                Basis::Emissions => emissions_kg,
                Basis::Energy => energy_kwh,
            } / f.per_unit,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header CodeCarbon 3.3.2 wrote in a `scripts/smoke-test.sh` run,
    /// with that run's row.
    const REAL_HEADER: &str = "timestamp,project_name,run_id,experiment_id,duration,emissions,emissions_rate,cpu_power,gpu_power,ram_power,cpu_energy,gpu_energy,ram_energy,energy_consumed,water_consumed,country_name,country_iso_code,region,cloud_provider,cloud_region,os,python_version,codecarbon_version,cpu_count,cpu_model,gpu_count,gpu_model,longitude,latitude,ram_total_size,tracking_mode,cpu_utilization_percent,gpu_utilization_percent,ram_utilization_percent,ram_used_gb,on_cloud,pue,wue";
    const REAL_ROW: &str = "2026-11-02T22:53:44,codecarbon-gui,0d5a9d75-6864-439c-84a0-695890b7241f,5b0fa12a-3dd7-45bb-9766-cc326314d9f1,3.677586947000009,6.634566819475895e-07,1.8040543745370976e-07,1.6013363036406787,0.0,10.0,1.6351477755446913e-06,0.0,1.0204047819444325e-05,1.1839195594989015e-05,0.0,France,FRA,,,,Linux-6.18.44-fc-v185-x86_64-with-glibc2.39,3.13.18,3.3.2,4,Intel(R) Xeon(R) Processor @ 2.10GHz,0,,,,15.714027404785156,machine,1.0,0,4.766666666666667,0.7495803833007812,N,1.0,0.0";

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, ISO_LOCAL).unwrap()
    }

    fn csv_of(rows: &[&str]) -> String {
        let mut text = String::from(REAL_HEADER);
        for r in rows {
            text.push('\n');
            text.push_str(r);
        }
        text.push('\n');
        text
    }

    /// A row in the real format: ended at `ts`, lasting `duration` s.
    fn row(ts: &str, duration: f64, emissions: f64, energy: f64) -> String {
        format!(
            "{ts},codecarbon-gui,run-{ts},exp,{duration},{emissions},0,10,0,5,0,0,0,{energy},0,France,FRA,,,,Linux,3.13,3.3.2,4,CPU,0,,,,16,machine,1,0,5,1,N,1.0,0.0"
        )
    }

    fn session(started: &str, emissions_kg: f64, energy_kwh: f64, duration_s: f64) -> Session {
        Session {
            started_at: dt(started),
            duration_s,
            energy_kwh,
            emissions_kg,
            avg_power_w: 0.0,
            country_name: None,
            project_name: "p".into(),
            run_id: started.into(),
        }
    }

    #[test]
    fn parses_codecarbon_real_header() {
        let (sessions, skipped) = parse(csv_of(&[REAL_ROW]).as_bytes()).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        // timestamp is the stop time: 22:53:44 − 3.68 s.
        assert_eq!(s.started_at, dt("2026-11-02T22:53:40"));
        assert_eq!(s.duration_s, 3.677586947000009);
        assert_eq!(s.energy_kwh, 1.1839195594989015e-05);
        assert_eq!(s.emissions_kg, 6.634566819475895e-07);
        assert!((s.avg_power_w - 11.59).abs() < 0.01, "{}", s.avg_power_w);
        assert_eq!(s.country_name.as_deref(), Some("France"));
        assert_eq!(s.project_name, "codecarbon-gui");
        assert_eq!(s.run_id, "0d5a9d75-6864-439c-84a0-695890b7241f");
        let json = serde_json::to_value(s).unwrap();
        assert_eq!(json["started_at"], "2026-11-02T22:53:40");
    }

    #[test]
    fn header_order_and_unknown_columns_do_not_matter() {
        let text = "future_column,energy_consumed,emissions,EXTRA,timestamp,duration,run_id\n\
                    x,0.002,0.0001,y,2026-03-01T10:00:00.123,60,r1\n";
        let (sessions, skipped) = parse(text.as_bytes()).unwrap();
        assert_eq!(skipped, 0);
        let s = &sessions[0];
        assert_eq!(
            s.started_at,
            dt("2026-03-01T09:59:00") + TimeDelta::milliseconds(123)
        );
        assert_eq!(s.energy_kwh, 0.002);
        assert!((s.avg_power_w - 120.0).abs() < 1e-9);
        assert_eq!(s.country_name, None, "missing optional column");
        assert_eq!(s.project_name, "", "missing optional column");
    }

    #[test]
    fn malformed_rows_are_skipped_and_counted() {
        let good = row("2026-03-02T10:00:00", 10.0, 0.001, 0.01);
        let text = csv_of(&[
            &good,
            "2026-03-02T11:00:00,too,few,fields",
            &row("not a date", 10.0, 0.001, 0.01),
            &row("2026-03-02T12:00:00", 10.0, f64::NAN, 0.01),
            &row("2026-03-02T12:00:00", -5.0, 0.001, 0.01),
            &good.replace(",0.001,", ",abc,"),
            &row("2026-03-02 13:00:00", 0.0, 0.0, 0.0),
        ]);
        let (sessions, skipped) = parse(text.as_bytes()).unwrap();
        assert_eq!(skipped, 5);
        assert_eq!(sessions.len(), 2);
        // A zero-length session: the power falls back to the components.
        assert_eq!(sessions[0].started_at, dt("2026-03-02T13:00:00"));
        assert_eq!(sessions[0].avg_power_w, 15.0);
    }

    #[test]
    fn missing_required_column_skips_every_row() {
        let text = "timestamp,duration,emissions\n2026-03-02T10:00:00,1,0.1\n";
        assert_eq!(parse(text.as_bytes()).unwrap(), (Vec::new(), 1));
    }

    #[test]
    fn newest_first() {
        let text = csv_of(&[
            &row("2026-03-01T10:00:00", 60.0, 0.001, 0.01),
            &row("2026-03-03T10:00:00", 60.0, 0.001, 0.01),
            &row("2026-03-02T10:00:00", 60.0, 0.001, 0.01),
        ]);
        let (sessions, _) = parse(text.as_bytes()).unwrap();
        let days: Vec<_> = sessions.iter().map(|s| s.started_at.day()).collect();
        assert_eq!(days, [3, 2, 1]);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("cc-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = load(&dir.join(CSV_FILE)).unwrap();
        assert!(h.sessions.is_empty());
        assert_eq!(h.skipped_rows, 0);

        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CSV_FILE);
        std::fs::write(&path, csv_of(&[REAL_ROW, "garbage"])).unwrap();
        let h = load(&path).unwrap();
        assert_eq!((h.sessions.len(), h.skipped_rows), (1, 1));
        assert!(h.path.ends_with(CSV_FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn week_and_month_boundaries() {
        let sessions = [
            session("2026-11-02T08:00:00", 1.0, 10.0, 60.0), // Monday
            session("2026-11-01T23:59:59", 2.0, 20.0, 60.0), // Sunday
            session("2026-10-31T23:59:59", 32.0, 320.0, 60.0), // Saturday, October
            session("2026-10-26T00:00:00", 4.0, 40.0, 60.0), // previous Monday
            session("2026-11-08T12:00:00", 8.0, 80.0, 60.0), // in the future (clock change)
            session("2025-11-02T08:00:00", 16.0, 160.0, 60.0), // a year ago
        ];

        // Monday 2 November 2026: the ISO week starts today.
        let s = summarise(&sessions, dt("2026-11-02T09:00:00"));
        assert_eq!(s.today.sessions, 1);
        assert_eq!(s.today.emissions_kg, 1.0);
        assert_eq!(s.this_week.sessions, 1, "the week starts today");
        assert_eq!(s.this_week.emissions_kg, 1.0);
        assert_eq!(s.this_month.sessions, 2);
        assert_eq!(
            s.this_month.emissions_kg, 3.0,
            "not October, a year ago or the future"
        );
        assert_eq!(s.all_time.sessions, 6);
        assert_eq!(s.all_time.energy_kwh, 630.0);
        assert_eq!(s.all_time.duration_s, 360.0);

        // Sunday 1 November: the week started on Monday 26 October.
        let s = summarise(&sessions, dt("2026-11-01T23:59:59"));
        assert_eq!(s.today.emissions_kg, 2.0);
        assert_eq!(s.this_week.emissions_kg, 38.0);
        assert_eq!(s.this_month.emissions_kg, 2.0);

        // Saturday 31 October.
        let s = summarise(&sessions, dt("2026-10-31T12:00:00"));
        assert_eq!(
            s.today.emissions_kg, 32.0,
            "a session counts for the day it started"
        );
        assert_eq!(s.this_week.emissions_kg, 36.0);
        assert_eq!(s.this_month.emissions_kg, 36.0);
    }

    #[test]
    fn daily_series_is_zero_filled() {
        let now = dt("2026-03-01T12:00:00");
        let sessions = [
            session("2026-03-01T08:00:00", 1.0, 10.0, 60.0),
            session("2026-03-01T09:00:00", 1.0, 10.0, 60.0),
            session("2026-02-28T09:00:00", 3.0, 30.0, 30.0),
            session("2026-01-31T09:00:00", 5.0, 50.0, 60.0), // first day of the window
            session("2026-01-30T09:00:00", 7.0, 70.0, 60.0), // outside
        ];
        let s = summarise(&sessions, now);
        assert_eq!(s.daily.len(), DAILY_DAYS as usize);
        assert_eq!(s.daily[0].date, "2026-01-31");
        assert_eq!(s.daily[0].emissions_kg, 5.0);
        assert_eq!(s.daily[28].date, "2026-02-28");
        assert_eq!(s.daily[28].duration_s, 30.0);
        let last = s.daily.last().unwrap();
        assert_eq!(last.date, "2026-03-01");
        assert_eq!(last.emissions_kg, 2.0);
        assert_eq!(last.energy_kwh, 20.0);
        assert_eq!(s.daily.iter().filter(|d| d.emissions_kg == 0.0).count(), 27);

        let empty = summarise(&[], now);
        assert_eq!(empty.daily.len(), DAILY_DAYS as usize);
        assert!(empty.daily.iter().all(|d| d.emissions_kg == 0.0));
        assert_eq!(empty.all_time, Totals::default());
    }

    #[test]
    fn equivalents_use_the_factor_table() {
        let e = equivalents(1.7, 0.5);
        let keys: Vec<_> = e.iter().map(|x| x.key).collect();
        assert_eq!(
            keys,
            [
                "car_km",
                "smartphone_charges",
                "led_tv_hours",
                "streaming_hours"
            ]
        );
        assert!((e[0].value - 10.0).abs() < 1e-9);
        assert_eq!(e[0].unit, "km");
        assert!((e[1].value - 212.5).abs() < 1e-9);
        assert!((e[2].value - 5.0).abs() < 1e-9, "TV uses the energy");
        assert!((e[3].value - 1.7 / 0.055).abs() < 1e-9);

        assert!(equivalents(f64::NAN, -1.0).iter().all(|x| x.value == 0.0));
        for f in FACTORS {
            assert!(f.per_unit > 0.0);
        }
        let json = serde_json::to_value(&e[0]).unwrap();
        assert_eq!(json["key"], "car_km");
        assert_eq!(json["unit"], "km");
    }
}
