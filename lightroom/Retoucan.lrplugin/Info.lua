-- Retoucan for Lightroom Classic. Installed and updated by the Retoucan app.
return {
  LrSdkVersion = 12.0,
  LrSdkMinimumVersion = 10.0,
  LrToolkitIdentifier = 'com.joemalott.retoucan',
  LrPluginName = 'Retoucan',
  LrPluginInfoUrl = 'https://github.com/joemalott/retoucan',
  LrInitPlugin = 'Init.lua',
  LrShutdownPlugin = 'Shutdown.lua',
  LrLibraryMenuItems = {
    { title = 'Edit Selected Photos with Retoucan', file = 'EditSelected.lua', enabledWhen = 'photosSelected' },
    { title = 'Apply Waiting Retoucan Edits Now', file = 'ApplyNow.lua' },
  },
  LrExportMenuItems = {
    { title = 'Edit Selected Photos with Retoucan', file = 'EditSelected.lua', enabledWhen = 'photosSelected' },
  },
  VERSION = { major = 0, minor = 1, revision = 0 },
}
