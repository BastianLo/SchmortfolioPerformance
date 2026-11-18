import { BarChart, LineChart, PieChart, TreemapChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  TooltipComponent,
} from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import * as React from "react";
import { cssVar, useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

echarts.use([
  LineChart,
  BarChart,
  PieChart,
  TreemapChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  DataZoomComponent,
  MarkLineComponent,
  CanvasRenderer,
]);

export type EChartsOption = echarts.EChartsCoreOption;

export const PALETTE = [
  "#6366f1",
  "#10b981",
  "#f59e0b",
  "#ef4444",
  "#0ea5e9",
  "#a855f7",
  "#14b8a6",
  "#f97316",
  "#84cc16",
  "#ec4899",
];

export function chartTheme() {
  return {
    fg: cssVar("--fg"),
    muted: cssVar("--muted"),
    border: cssVar("--border"),
    surface: cssVar("--surface"),
    accent: cssVar("--accent"),
    pos: cssVar("--pos"),
    neg: cssVar("--neg"),
  };
}

export function EChart({
  option,
  className,
  onClick,
}: {
  option: (t: ReturnType<typeof chartTheme>) => EChartsOption;
  className?: string;
  onClick?: (params: any) => void;
}) {
  const ref = React.useRef<HTMLDivElement>(null);
  const chart = React.useRef<echarts.ECharts | null>(null);
  const { dark } = useTheme();

  React.useEffect(() => {
    if (!ref.current) return;
    const c = echarts.init(ref.current, undefined, { renderer: "canvas" });
    chart.current = c;
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      c.dispose();
      chart.current = null;
    };
  }, []);

  // `dark` is not read here, but the theme colors (CSS variables) change with it
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-render on theme change
  React.useEffect(() => {
    const t = chartTheme();
    chart.current?.setOption(
      {
        color: PALETTE,
        textStyle: { fontFamily: "inherit", color: t.muted },
        animationDuration: 400,
        ...option(t),
      },
      { notMerge: true },
    );
  }, [option, dark]);

  React.useEffect(() => {
    const c = chart.current;
    if (!c || !onClick) return;
    c.on("click", onClick);
    return () => {
      c.off("click", onClick);
    };
  }, [onClick]);

  return <div ref={ref} className={cn("h-64 w-full", className)} />;
}

export function tooltipStyle(t: ReturnType<typeof chartTheme>) {
  return {
    backgroundColor: t.surface,
    borderColor: t.border,
    textStyle: { color: t.fg, fontSize: 12 },
    extraCssText: "border-radius:10px;box-shadow:0 8px 24px rgba(0,0,0,.12);",
  };
}
