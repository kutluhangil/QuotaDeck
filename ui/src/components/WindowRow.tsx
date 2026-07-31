import type { ReactNode } from "react";

import { formatPercent, type Level } from "../format";
import { useLocale } from "../store";
import type { Confidence } from "../types";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { UsageBar } from "./UsageBar";

/**
 * One limit, on one line: what it covers, how full it is, and when it lets go.
 *
 * Every window a provider reports gets the same four columns, and so does the pace projection.
 * The old card gave its worst window a 28px display number and demoted the rest to a bare list,
 * which made two windows of the same limit look like two different kinds of fact. They are not
 * — a weekly ceiling stops the work exactly as hard as a five-hour one.
 *
 * The columns are a grid rather than flex so the percentages line up down the card whatever the
 * labels are; with tabular figures that column never moves as the numbers tick.
 */
export function WindowRow({
  label,
  spokenLabel,
  percent,
  level,
  confidence,
  meta,
  kind = "window",
}: {
  /** Short enough for the first column: a window length, or the word "Pace". */
  label: string;
  /** What a screen reader hears instead — the window's kind, or the projection named as one. */
  spokenLabel: string;
  percent: number | null;
  /** Overrides the ramp for the pace row, whose bands are wider on purpose. */
  level?: Level;
  /** Absent on the pace row: a projection has no source to cite. */
  confidence?: Confidence;
  /** Right-hand cell — a countdown, or what the projection lands on. */
  meta?: ReactNode;
  kind?: "window" | "pace";
}) {
  const locale = useLocale();

  return (
    <li className="row" data-kind={kind}>
      <span className="type-caption row__label">
        {confidence && <ConfidenceBadge confidence={confidence} compact />}
        {label}
      </span>
      <span className="row__bar">
        {percent === null ? (
          <span className="row__bar-empty" />
        ) : (
          <UsageBar percent={percent} windowName={spokenLabel} level={level} />
        )}
      </span>
      {/* The meter beside it already announces this number with its bounds. */}
      <span className="type-metric row__value" aria-hidden="true">
        {percent === null ? "—" : formatPercent(percent, locale)}
      </span>
      <span className="type-caption row__meta">{meta}</span>
    </li>
  );
}
