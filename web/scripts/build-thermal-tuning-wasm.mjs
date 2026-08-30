import { execFileSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoRoot = path.resolve(webRoot, '..')
const outputDirectory = path.join(webRoot, 'src/generated/thermal-tuning-wasm')

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
