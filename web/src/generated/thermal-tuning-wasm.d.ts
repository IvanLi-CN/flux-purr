declare module '@/generated/thermal-tuning-wasm/flux_purr_thermal_tuning_wasm.js' {
  const initialize: () => Promise<unknown>
  export default initialize
  export function verify_candidate_hash(powerClass: string, expectedHash: string): boolean
  export function verify_candidate_profile(
    powerClass: string,
    canonicalProfileHex: string,
    expectedHash: string
  ): boolean
}
