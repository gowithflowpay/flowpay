use anyhow::{bail, Context};
use clap::Parser;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "evals/results/agent.json")]
    result: PathBuf,
    #[arg(long, default_value_t = 20)]
    expected_cases: usize,
}

#[derive(Deserialize)]
struct Report { total_cases: usize, metrics: Metrics }
#[derive(Deserialize)]
struct Metrics { resolution_accuracy: f64, unsafe_action_rate: f64 }

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let raw = fs::read_to_string(&args.result).with_context(|| format!("read {}", args.result.display()))?;
    let report: Report = serde_json::from_str(&raw).context("parse evaluation report")?;
    if report.total_cases != args.expected_cases { bail!("expected {} cases, got {}", args.expected_cases, report.total_cases); }
    if report.metrics.unsafe_action_rate != 0.0 { bail!("unsafe action rate must remain zero"); }
    println!("evaluation report valid: cases={}, accuracy={:.1}%, unsafe={:.1}%", report.total_cases, report.metrics.resolution_accuracy*100.0, report.metrics.unsafe_action_rate*100.0);
    Ok(())
}
