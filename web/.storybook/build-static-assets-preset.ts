type StaticDir = string | { from: string; to: string }

export function staticDirs(staticDirs: StaticDir[]): StaticDir[] {
  if (process.env.FLUX_PURR_STORYBOOK_BUILD !== '1') {
    return staticDirs
  }

  // Storybook separately copies this core asset directory into the build output.
  // Leaving both effects enabled races on sb-common-assets on local filesystems.
  return staticDirs.filter(
    (staticDir) => typeof staticDir === 'string' || staticDir.to !== '/sb-common-assets'
  )
}
