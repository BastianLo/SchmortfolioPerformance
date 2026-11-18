import * as React from "react";
import type { Schemas } from "@/api/client";
import { AMOUNT, date as fmtDate, money, monthYear, percent } from "@/lib/format";
import { EChart, PALETTE, tooltipStyle } from "./echart";

type Point = Schemas["SeriesPoint"];

function axisCommon(t: { muted: string; border: string }) {
  return {
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: { color: t.muted, fontSize: 11 },
    splitLine: { lineStyle: { color: t.border, type: "dashed" as const } },
  };
}

export function ValueChart({
  series,
  currency,
  mode,
  className,
}: {
  series: Point[];
  currency: string;
  mode: "value" | "return" | "drawdown";
  className?: string;
}) {
  const option = React.useCallback(
    (t: ReturnType<typeof import("./echart").chartTheme>) => {
      const dates = series.map((p) => p.date);
      const common = {
        grid: { left: 8, right: 12, top: 16, bottom: 28, containLabel: true },
        tooltip: {
          trigger: "axis",
          ...tooltipStyle(t),
          axisPointer: { type: "line", lineStyle: { color: t.muted, type: "dashed" } },
        },
        xAxis: {
          type: "category",
          data: dates,
          boundaryGap: false,
          ...axisCommon(t),
          splitLine: { show: false },
          axisLabel: {
            color: t.muted,
            fontSize: 11,
            hideOverlap: true,
            formatter: (v: string) => monthYear(v),
          },
        },
      };
      if (mode === "value") {
        return {
          ...common,
          tooltip: {
            ...common.tooltip,
            formatter: (ps: { axisValue: string; seriesName: string; value: number; color: string }[]) =>
              `<div style="font-weight:600;margin-bottom:4px">${fmtDate(ps[0].axisValue)}</div>${ps
                .map(
                  (p) =>
                    `<div style="display:flex;justify-content:space-between;gap:16px"><span><span style="display:inline-block;width:8px;height:8px;border-radius:9px;background:${p.color};margin-right:6px"></span>${p.seriesName}</span><b>${money(p.value * AMOUNT, currency)}</b></div>`,
                )
                .join("")}`,
          },
          yAxis: {
            type: "value",
            scale: true,
            ...axisCommon(t),
            axisLabel: {
              color: t.muted,
              fontSize: 11,
              formatter: (v: number) => money(v * AMOUNT, currency, { compact: true }),
            },
          },
          series: [
            {
              name: "Vermögen",
              type: "line",
              data: series.map((p) => p.total / AMOUNT),
              showSymbol: false,
              smooth: 0.2,
              lineStyle: { width: 2, color: t.accent },
              itemStyle: { color: t.accent },
              areaStyle: {
                color: {
                  type: "linear",
                  x: 0,
                  y: 0,
                  x2: 0,
                  y2: 1,
                  colorStops: [
                    { offset: 0, color: `${t.accent}55` },
                    { offset: 1, color: `${t.accent}00` },
                  ],
                },
              },
            },
            {
              name: "Investiert",
              type: "line",
              step: "end",
              data: series.map((p) => p.invested / AMOUNT),
              showSymbol: false,
              lineStyle: { width: 1.5, color: t.muted, type: "dashed" },
              itemStyle: { color: t.muted },
            },
          ],
        };
      }
      const values = series.map((p) => (mode === "return" ? p.ttwror : p.drawdown));
      const color = mode === "return" ? t.accent : t.neg;
      return {
        ...common,
        tooltip: {
          ...common.tooltip,
          formatter: (ps: { axisValue: string; value: number }[]) =>
            `<div style="font-weight:600;margin-bottom:4px">${fmtDate(ps[0].axisValue)}</div>${mode === "return" ? "TTWROR" : "Drawdown"}: <b>${percent(ps[0].value, { sign: true })}</b>`,
        },
        yAxis: {
          type: "value",
          ...axisCommon(t),
          axisLabel: { color: t.muted, fontSize: 11, formatter: (v: number) => percent(v, { digits: 0 }) },
        },
        series: [
          {
            type: "line",
            data: values,
            showSymbol: false,
            smooth: 0.2,
            lineStyle: { width: 2, color },
            itemStyle: { color },
            areaStyle: { color: `${color}22`, origin: 0 },
            markLine:
              mode === "return"
                ? {
                    silent: true,
                    symbol: "none",
                    lineStyle: { color: t.border },
                    data: [{ yAxis: 0 }],
                    label: { show: false },
                  }
                : undefined,
          },
        ],
      };
    },
    [series, currency, mode],
  );
  return <EChart option={option} className={className} />;
}

export function Donut({
  data,
  currency,
  className,
  center,
}: {
  data: { name: string; value: number; color?: string | null }[];
  currency: string;
  className?: string;
  center?: string;
}) {
  const option = React.useCallback(
    (t: ReturnType<typeof import("./echart").chartTheme>) => ({
      tooltip: {
        trigger: "item",
        ...tooltipStyle(t),
        formatter: (p: { name: string; value: number; percent: number }) =>
          `${p.name}<br/><b>${money(p.value * AMOUNT, currency)}</b> · ${p.percent.toLocaleString("de-DE")} %`,
      },
      title: center
        ? { text: center, left: "center", top: "center", textStyle: { color: t.fg, fontSize: 13, fontWeight: 600 } }
        : undefined,
      series: [
        {
          type: "pie",
          radius: ["58%", "85%"],
          avoidLabelOverlap: true,
          itemStyle: { borderColor: t.surface, borderWidth: 2, borderRadius: 4 },
          label: { show: false },
          data: data.map((d, i) => ({
            name: d.name,
            value: d.value / AMOUNT,
            itemStyle: { color: d.color || PALETTE[i % PALETTE.length] },
          })),
        },
      ],
    }),
    [data, currency, center],
  );
  return <EChart option={option} className={className} />;
}

export function BarsChart({
  categories,
  series,
  currency,
  className,
  percentMode,
  stacked,
}: {
  categories: string[];
  series: { name: string; data: number[]; color?: string }[];
  currency?: string;
  className?: string;
  percentMode?: boolean;
  stacked?: boolean;
}) {
  const option = React.useCallback(
    (t: ReturnType<typeof import("./echart").chartTheme>) => {
      const fmt = (v: number) => (percentMode ? percent(v, { sign: true }) : money(v * AMOUNT, currency ?? "EUR"));
      return {
        grid: { left: 8, right: 12, top: series.length > 1 ? 32 : 16, bottom: 24, containLabel: true },
        legend:
          series.length > 1
            ? { top: 0, textStyle: { color: t.muted }, icon: "roundRect", itemWidth: 10, itemHeight: 10 }
            : undefined,
        tooltip: {
          trigger: "axis",
          ...tooltipStyle(t),
          axisPointer: { type: "shadow" },
          valueFormatter: fmt,
        },
        xAxis: { type: "category", data: categories, ...axisCommon(t), splitLine: { show: false } },
        yAxis: {
          type: "value",
          ...axisCommon(t),
          axisLabel: {
            color: t.muted,
            fontSize: 11,
            formatter: (v: number) =>
              percentMode
                ? percent(v, { digits: Math.abs(v) < 0.1 && v !== 0 ? 1 : 0 })
                : money(v * AMOUNT, currency ?? "EUR", { compact: true }),
          },
        },
        series: series.map((s) => ({
          name: s.name,
          type: "bar",
          stack: stacked ? "total" : undefined,
          barMaxWidth: 28,
          data: s.data.map((v) =>
            percentMode
              ? { value: v, itemStyle: { color: v >= 0 ? t.pos : t.neg, borderRadius: [3, 3, 0, 0] } }
              : { value: v / AMOUNT },
          ),
          itemStyle: s.color ? { color: s.color, borderRadius: stacked ? 0 : [3, 3, 0, 0] } : undefined,
        })),
      };
    },
    [categories, series, currency, percentMode, stacked],
  );
  return <EChart option={option} className={className} />;
}

export function PriceChart({
  prices,
  currency,
  markers,
  className,
}: {
  prices: { date: string; value: number }[];
  currency?: string | null;
  markers?: { date: string; kind: "buy" | "sell" | "dividend"; label: string }[];
  className?: string;
}) {
  const option = React.useCallback(
    (t: ReturnType<typeof import("./echart").chartTheme>) => {
      const dates = prices.map((p) => p.date);
      const idx = new Map(dates.map((d, i) => [d, i]));
      const nearest = (d: string) => {
        if (idx.has(d)) return d;
        const i = dates.findIndex((x) => x >= d);
        return i >= 0 ? dates[i] : dates[dates.length - 1];
      };
      const byKind = (kind: string) =>
        (markers ?? [])
          .filter((m) => m.kind === kind)
          .map((m) => {
            const d = nearest(m.date);
            const v = prices[idx.get(d) ?? 0]?.value / 1e8;
            return { value: [d, v], name: m.label };
          });
      return {
        grid: { left: 8, right: 12, top: 16, bottom: 56, containLabel: true },
        tooltip: {
          trigger: "axis",
          ...tooltipStyle(t),
          valueFormatter: (v: number) =>
            typeof v === "number"
              ? v.toLocaleString("de-DE", { style: currency ? "currency" : "decimal", currency: currency ?? undefined })
              : v,
        },
        dataZoom: [
          { type: "inside", start: 0, end: 100 },
          {
            type: "slider",
            height: 20,
            bottom: 8,
            borderColor: t.border,
            textStyle: { color: t.muted },
            fillerColor: `${t.accent}22`,
          },
        ],
        xAxis: {
          type: "category",
          data: dates,
          boundaryGap: false,
          ...axisCommon(t),
          splitLine: { show: false },
          axisLabel: { color: t.muted, fontSize: 11, hideOverlap: true, formatter: (v: string) => monthYear(v) },
        },
        yAxis: { type: "value", scale: true, ...axisCommon(t) },
        series: [
          {
            name: "Kurs",
            type: "line",
            data: prices.map((p) => p.value / 1e8),
            showSymbol: false,
            lineStyle: { width: 1.5, color: t.accent },
            itemStyle: { color: t.accent },
            areaStyle: { color: `${t.accent}18` },
            sampling: "lttb",
          },
          {
            name: "Kauf",
            type: "scatter" as const,
            data: byKind("buy"),
            symbolSize: 9,
            itemStyle: { color: t.pos },
            tooltip: { show: false },
          },
          {
            name: "Verkauf",
            type: "scatter" as const,
            data: byKind("sell"),
            symbolSize: 9,
            itemStyle: { color: t.neg },
            tooltip: { show: false },
          },
        ],
      };
    },
    [prices, currency, markers],
  );
  return <EChart option={option} className={className} />;
}
