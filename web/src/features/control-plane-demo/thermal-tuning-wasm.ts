type ThermalTuningWasmModule = {
  default: () => Promise<unknown>
  verify_candidate_hash: (powerClass: string, expectedHash: string) => boolean
  verify_candidate_profile: (
    powerClass: string,
    canonicalProfileHex: string,
    expectedHash: string
  ) => boolean
}

let modulePromise: Promise<ThermalTuningWasmModule> | undefined

function loadThermalTuningWasm() {
  modulePromise ??= import('@/generated/thermal-tuning-wasm/flux_purr_thermal_tuning_wasm.js').then(
    async (module) => {
      await module.default()
      return module
    }
  )
  return modulePromise
}

/** Returns null when the browser cannot initialize the generated Wasm module. */
export async function verifyThermalTuningCandidate(
  powerClass: 'pps3a' | 'pps5a',
  canonicalProfileHex: string,
  candidateHash: string
): Promise<boolean | null> {
  try {
    const wasm = await loadThermalTuningWasm()
    return wasm.verify_candidate_profile(powerClass, canonicalProfileHex, candidateHash)
  } catch {
    return null
  }
}
