export function reconcileOscToolbarLayout({
  container,
  buttons,
  configured,
  previousFingerprint,
}) {
  const fingerprint = configured.join(",");
  if (fingerprint === previousFingerprint) return fingerprint;

  for (const button of buttons.values()) button.hidden = true;
  for (const buttonType of configured) {
    const button = buttons.get(buttonType);
    if (!button) continue;
    button.hidden = false;
    container.append(button);
  }
  container.style.width = `${Math.max(24, configured.length * (configured.length === 5 ? 20 : 24))}px`;
  return fingerprint;
}
