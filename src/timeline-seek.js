export function iinaTimelineSeekPlan(value, maximum, followGlobalSeekType, globalSeekType) {
  const sliderMaximum = Number(maximum);
  const sliderValue = Number(value);
  const percent = Number.isFinite(sliderValue) && Number.isFinite(sliderMaximum) && sliderMaximum > 0
    ? 100 * sliderValue / sliderMaximum
    : 0;
  // Preference.bool(for: .useExactSeek) coerces IINA's 0/1/2 enum to
  // false/true/true. When followGlobal... is disabled, the slider forces exact.
  const exact = !Boolean(followGlobalSeekType) || Number(globalSeekType) !== 0;
  return { type: "seek-percent", percent, exact };
}
