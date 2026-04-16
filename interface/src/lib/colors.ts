// Central color mappings for the interface.
//
// The pigment vocabulary is a small palette of Tailwind color names —
// blue, pink, amber, purple, green, cyan, orange, red, violet, indigo.
// Each domain (MemoryType, cortex event category, messaging platform)
// maps a typed key onto one of those names, and the helpers here
// derive both the Tailwind class pair (`bg-*/15 text-*-400`) and the
// raw hex (for recharts / non-Tailwind consumers) from the same source.
//
// Keep additions here rather than re-declaring another ad-hoc map at
// a call site — that's the whole point of this file.

import type {MemoryType} from "@/api/client";

type Pigment =
	| "blue"
	| "pink"
	| "amber"
	| "purple"
	| "green"
	| "cyan"
	| "orange"
	| "red"
	| "violet"
	| "indigo"
	| "gray";

const PIGMENT_HEX: Record<Pigment, string> = {
	blue: "#3b82f6",
	pink: "#ec4899",
	amber: "#f59e0b",
	purple: "#8b5cf6",
	green: "#10b981",
	cyan: "#06b6d4",
	orange: "#f97316",
	red: "#ef4444",
	violet: "#8b5cf6",
	indigo: "#6366f1",
	gray: "#6b7280",
};

/**
 * `bg-{color}-500/{alpha} text-{color}-400` pair. Use the Tailwind classes
 * directly rather than constructing them dynamically so Tailwind's source
 * scanner sees them at build time.
 */
type ClassPair = string;

const PIGMENT_CLASS_15: Record<Pigment, ClassPair> = {
	blue: "bg-blue-500/15 text-blue-400",
	pink: "bg-pink-500/15 text-pink-400",
	amber: "bg-amber-500/15 text-amber-400",
	purple: "bg-purple-500/15 text-purple-400",
	green: "bg-green-500/15 text-green-400",
	cyan: "bg-cyan-500/15 text-cyan-400",
	orange: "bg-orange-500/15 text-orange-400",
	red: "bg-red-500/15 text-red-400",
	violet: "bg-violet-500/15 text-violet-400",
	indigo: "bg-indigo-500/15 text-indigo-400",
	gray: "bg-gray-500/15 text-gray-400",
};

const PIGMENT_CLASS_20: Record<Pigment, ClassPair> = {
	blue: "bg-blue-500/20 text-blue-400",
	pink: "bg-pink-500/20 text-pink-400",
	amber: "bg-amber-500/20 text-amber-400",
	purple: "bg-purple-500/20 text-purple-400",
	green: "bg-green-500/20 text-green-400",
	cyan: "bg-cyan-500/20 text-cyan-400",
	orange: "bg-orange-500/20 text-orange-400",
	red: "bg-red-500/20 text-red-400",
	violet: "bg-violet-500/20 text-violet-400",
	indigo: "bg-indigo-500/20 text-indigo-400",
	gray: "bg-gray-500/20 text-gray-400",
};

// --- MemoryType ---

const MEMORY_TYPE_PIGMENT: Record<MemoryType, Pigment> = {
	fact: "blue",
	preference: "pink",
	decision: "amber",
	identity: "purple",
	event: "green",
	observation: "cyan",
	goal: "orange",
	todo: "red",
};

export function memoryTypeClass(type: MemoryType): ClassPair {
	return PIGMENT_CLASS_15[MEMORY_TYPE_PIGMENT[type]];
}

export function memoryTypeHex(type: MemoryType): string {
	return PIGMENT_HEX[MEMORY_TYPE_PIGMENT[type]];
}

/**
 * Hex list in `MemoryType` declaration order — handy for charts that
 * iterate a palette by index (e.g. recharts Pie `Cell` arrays).
 */
export const MEMORY_TYPE_HEX_PALETTE: readonly string[] = Object.values(
	MEMORY_TYPE_PIGMENT,
).map((p) => PIGMENT_HEX[p]);

// --- Cortex event category ---

const EVENT_CATEGORY_PIGMENT: Record<string, Pigment> = {
	bulletin_generated: "blue",
	bulletin_failed: "red",
	maintenance_run: "green",
	memory_merged: "green",
	memory_decayed: "green",
	memory_pruned: "green",
	association_created: "violet",
	contradiction_flagged: "violet",
	worker_killed: "amber",
	branch_killed: "amber",
	circuit_breaker_tripped: "amber",
	observation_created: "cyan",
	health_check: "blue",
};

export function eventCategoryClass(eventType: string): ClassPair {
	const pigment = EVENT_CATEGORY_PIGMENT[eventType];
	if (!pigment) return "bg-app-dark-box text-ink-faint";
	return PIGMENT_CLASS_15[pigment];
}

// --- Messaging platform ---

const PLATFORM_PIGMENT: Record<string, Pigment> = {
	discord: "indigo",
	slack: "green",
	telegram: "blue",
	twitch: "purple",
	cron: "amber",
};

export function platformColor(platform: string): ClassPair {
	const pigment = PLATFORM_PIGMENT[platform] ?? "gray";
	return PIGMENT_CLASS_20[pigment];
}
