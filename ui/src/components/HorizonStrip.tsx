import { useMemo, useState } from "react";

import { formatClock, formatSpan, formatTokens, levelFor } from "../format";
import { axisFor, columns, historySeconds, type Column } from "../horizon";
import { strings } from "../strings";
import type { Bucket, QuotaWindow } from "../types";

/**
 * The signature element: one horizontal band per provider showing where the quota went.
 *
 * SVG rather than canvas. The columns carry design tokens that change with the theme, and a
 * canvas would have to read every one of them out of the cascade by hand and redraw on every
 * appearance change. At this column count the DOM cost is a rounding error next to that.
 *
 * There is no animation loop. The strip redraws when a snapshot arrives — every five seconds
 * while the panel is open — and the movement between two snapshots is a CSS transition. The
 * blueprint budgeted one frame a second; this spends less, and a menu bar app that keeps a
 * timer running is a menu bar app people uninstall.
 */

/** Drawing units. The panel is a fixed 380px, so these are very nearly device pixels. */
const WIDTH = 340;
const HEIGHT = 40;
/** The horizon line the columns stand on. */
const BASELINE = 1;
const SPAN = HEIGHT - BASELINE;

const PITCH = 4;
const BAR = 3;
const COUNT = Math.floor(WIDTH / PITCH);

export function HorizonStrip({
  window,
  series,
  now,
}: {
  window: QuotaWindow;
  series: Bucket[];
  /** Milliseconds, as the rest of the panel counts time. */
  now: number;
}) {
  const [hovered, setHovered] = useState<number | null>(null);

  const { axis, drawn } = useMemo(() => {
    const axis = axisFor(window, Math.floor(now / 1000));
    return { axis, drawn: columns(series, axis, COUNT) };
  }, [window, series, now]);

  const level = window.usedPercent === null ? "ample" : levelFor(window.usedPercent);
  const active = hovered === null ? null : (drawn[hovered] ?? null);
  const spent = drawn.reduce((total, column) => total + column.tokens, 0);

  function track(event: React.MouseEvent<SVGSVGElement>) {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width === 0) return;
    const at = ((event.clientX - box.left) / box.width) * COUNT;
    setHovered(Math.min(COUNT - 1, Math.max(0, Math.floor(at))));
  }

  return (
    <div className="strip" data-level={level}>
      <svg
        className="strip__plot"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        // The panel has one fixed width, so the horizontal stretch is imperceptible; letting
        // it scale keeps the strip correct in the wider dashboard window of Phase 7.
        preserveAspectRatio="none"
        role="img"
        aria-label={strings.strip.summary(
          formatSpan(historySeconds(axis)),
          formatTokens(spent),
        )}
        onMouseMove={track}
        onMouseLeave={() => setHovered(null)}
      >
        {/*
          The track the columns stand in. It is drawn rather than left blank so the strip has
          a stated extent: the left edge is where the window ends, and work that crosses it
          stops counting against the quota.
        */}
        <rect className="strip__track" x={0} y={0} width={WIDTH} height={SPAN} />

        {/*
          Which column the readout below is talking about. At this width a column is three
          pixels wide, so brightening the bar itself is not an answer — a full-height band is
          the only thing findable at a glance.
        */}
        {hovered !== null && (
          <rect
            className="strip__cursor"
            x={hovered * PITCH - 1}
            y={0}
            width={BAR + 2}
            height={SPAN}
          />
        )}

        {drawn.map((column, index) => (
          <ColumnBar key={column.start} column={column} x={index * PITCH} />
        ))}

        <line className="strip__horizon" x1={0} y1={SPAN} x2={WIDTH} y2={SPAN} />
      </svg>

      <p className="type-caption strip__axis">
        {active === null ? (
          <>
            <span className="strip__axis-start">
              {strings.strip.ago(formatSpan(historySeconds(axis)))}
            </span>
            <span className="strip__axis-end">{strings.strip.now}</span>
          </>
        ) : (
          <>
            <span className="strip__axis-start">{sliceLabel(active)}</span>
            <span className="strip__axis-end strip__reading">
              {active.tokens === 0
                ? strings.strip.quiet
                : strings.strip.tokens(formatTokens(active.tokens))}
            </span>
          </>
        )}
      </p>
    </div>
  );
}

/**
 * A column is a slice, not an instant. Over a weekly window one column is nearly two hours,
 * and showing a bare clock time there would claim a precision the column does not have.
 */
function sliceLabel(column: Column): string {
  const clock = formatClock(new Date(column.start * 1000).toISOString()) ?? "";
  if (column.seconds < 3_600) return clock;
  return `${clock} + ${formatSpan(column.seconds)}`;
}

function ColumnBar({ column, x }: { column: Column; x: number }) {
  return (
    <rect
      className="strip__bar"
      x={x}
      y={0}
      width={BAR}
      height={SPAN}
      // Scaling from the baseline rather than animating y and height keeps the movement to a
      // single transform, which is the one thing a compositor can do without a relayout.
      style={{ transform: `scaleY(${column.height})` }}
    />
  );
}
