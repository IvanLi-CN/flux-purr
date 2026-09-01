import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoRoot = path.resolve(webRoot, '..')
const outputDirectory = path.join(webRoot, 'src/generated/thermal-tuning-wasm')
const reportOutputDirectory = path.join(webRoot, 'src/generated/thermal-report')

mkdirSync(outputDirectory, { recursive: true })
execFileSync(
  'wasm-pack',
  [
    'build',
    path.join(repoRoot, 'crates/thermal-tuning-wasm'),
    '--target',
    'web',
    '--release',
    '--out-dir',
    outputDirectory,
    '--no-typescript',
  ],
  { cwd: repoRoot, stdio: 'inherit' }
)

mkdirSync(reportOutputDirectory, { recursive: true })
const reportTemplate = readFileSync(
  path.join(
    repoRoot,
    'tools/flux-purr-devd/src/bin/flux_purr/thermal_preliminary_review_template.html'
  ),
  'utf8'
)
  .replaceAll('{{', '{')
  .replaceAll('}}', '}')
writeFileSync(
  path.join(reportOutputDirectory, 'template.ts'),
  `// Generated from the canonical native thermal report template.\nexport const thermalReportTemplate = ${JSON.stringify(reportTemplate)}\n`
)
