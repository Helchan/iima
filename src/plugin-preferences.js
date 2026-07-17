export const IINA_DEFAULT_PLUGIN_REPOSITORIES = Object.freeze([
  Object.freeze({ repository: "iina/plugin-demo", identifier: "io.iina.demo" }),
  Object.freeze({ repository: "iina/plugin-online-media", identifier: "io.iina.ytdl" }),
  Object.freeze({ repository: "iina/plugin-userscript", identifier: "io.iina.userscript" }),
]);

export function defaultPluginRepositoryRows(installedPlugins = []) {
  const installedIdentifiers = new Set(
    Array.from(installedPlugins, (plugin) => String(plugin?.identifier || "")).filter(Boolean),
  );
  return IINA_DEFAULT_PLUGIN_REPOSITORIES.map((plugin) => ({
    ...plugin,
    installed: installedIdentifiers.has(plugin.identifier),
  }));
}

export function retainedPluginPreferenceSelection(selectedIdentifier, plugins = []) {
  const identifier = String(selectedIdentifier || "");
  return Array.from(plugins).some((plugin) => plugin?.identifier === identifier)
    ? identifier
    : null;
}

export function pluginReorderFinalIndex(sourceIndex, insertionIndex, pluginCount) {
  if (
    !Number.isInteger(sourceIndex)
    || !Number.isInteger(insertionIndex)
    || !Number.isInteger(pluginCount)
    || pluginCount < 1
    || sourceIndex < 0
    || sourceIndex >= pluginCount
    || insertionIndex < 0
    || insertionIndex > pluginCount
  ) return null;
  return sourceIndex < insertionIndex ? insertionIndex - 1 : insertionIndex;
}
