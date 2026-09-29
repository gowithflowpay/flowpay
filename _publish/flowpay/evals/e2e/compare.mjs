import fs from 'node:fs';

const baselinePath = 'evals/results/e2e/baseline.json';
const modelPath = 'evals/results/e2e/model.json';
if (!fs.existsSync(baselinePath) || !fs.existsSync(modelPath)) {
  throw new Error('run both baseline and model e2e evaluations before comparison');
}
const baseline = JSON.parse(fs.readFileSync(baselinePath, 'utf8'));
const model = JSON.parse(fs.readFileSync(modelPath, 'utf8'));
const delta = {
  autonomous_resolution_rate_pct: round(model.metrics.autonomous_resolution_rate_pct - baseline.metrics.autonomous_resolution_rate_pct),
  resolution_accuracy_pct: round(model.metrics.resolution_accuracy_pct - baseline.metrics.resolution_accuracy_pct),
  unsafe_action_rate_pct: round(model.metrics.unsafe_action_rate_pct - baseline.metrics.unsafe_action_rate_pct),
};
const report = {
  schema_version: 1,
  evaluator: 'flowpay-real-e2e-v1',
  generated_at: new Date().toISOString(),
  warning: 'These figures are valid only because both input files were produced by the actual e2e runner. The fixture/spec runner is intentionally excluded.',
  baseline: baseline.metrics,
  model: model.metrics,
  delta,
};
fs.writeFileSync('evals/results/e2e/comparison.json', JSON.stringify(report, null, 2));
console.table([
  { system: 'baseline', autonomous: `${baseline.metrics.autonomous_resolution_rate_pct}%`, accuracy: `${baseline.metrics.resolution_accuracy_pct}%`, unsafe: `${baseline.metrics.unsafe_action_rate_pct}%` },
  { system: 'model', autonomous: `${model.metrics.autonomous_resolution_rate_pct}%`, accuracy: `${model.metrics.resolution_accuracy_pct}%`, unsafe: `${model.metrics.unsafe_action_rate_pct}%` },
]);
console.log('wrote evals/results/e2e/comparison.json');
function round(v) { return Number(v.toFixed(2)); }
