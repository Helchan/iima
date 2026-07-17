export function invokePluginMpvHook(callback, payload, continueHook, reportError = () => {}) {
  let continued = false;
  const next = () => {
    if (continued) return;
    continued = true;
    try {
      const result = continueHook(payload);
      if (result && typeof result.catch === "function") {
        result.catch((error) => reportError("continue", error));
      }
    } catch (error) {
      reportError("continue", error);
    }
  };
  const isAsync = callback?.constructor?.name === "AsyncFunction";
  try {
    const result = callback(next);
    if (isAsync && result && typeof result.catch === "function") {
      result.catch((error) => reportError("async", error));
    }
  } catch (error) {
    reportError("callback", error);
  } finally {
    // IINA 1.3.5 auto-continues normal callbacks after they return. AsyncFunction callbacks own
    // the continuation and therefore must invoke next() explicitly when their work is complete.
    if (!isAsync) next();
  }
}
