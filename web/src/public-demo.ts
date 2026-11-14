export const isPublicDemoBuild = () =>
  import.meta.env.VITE_FLUX_PURR_PUBLIC_DEMO === '1' || import.meta.env.MODE === 'demo'
