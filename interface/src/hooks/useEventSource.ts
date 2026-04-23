import { useEffect, useRef, useState } from "react";
import { fetchEventSource } from "@microsoft/fetch-event-source";
import { authedFetch } from "@spacebot/api-client/authedFetch";

type EventHandler = (data: unknown) => void;

export type ConnectionState =
	| "connecting"
	| "connected"
	| "reconnecting"
	| "disconnected";

interface UseEventSourceOptions {
	/** Map of SSE event types to handlers */
	handlers: Record<string, EventHandler>;
	/** Whether to connect (default true) */
	enabled?: boolean;
	/** Called when the connection recovers after a disconnect */
	onReconnect?: () => void;
}

const INITIAL_RETRY_MS = 1000;
const MAX_RETRY_MS = 30_000;
const BACKOFF_MULTIPLIER = 2;

/**
 * SSE hook with exponential backoff, connection state tracking, and
 * reconnect notification for state recovery. Phase 6 PR C swapped the
 * transport from native EventSource to @microsoft/fetch-event-source
 * so the stream carries Authorization: Bearer <token> via authedFetch.
 * The public contract is unchanged: handlers dispatch by SSE event
 * name, onReconnect fires on recovery, and { connectionState } is
 * returned for the ConnectionBanner to render.
 */
export function useEventSource(url: string, options: UseEventSourceOptions) {
	const { handlers, enabled = true, onReconnect } = options;
	const handlersRef = useRef(handlers);
	handlersRef.current = handlers;

	const onReconnectRef = useRef(onReconnect);
	onReconnectRef.current = onReconnect;

	const [connectionState, setConnectionState] = useState<ConnectionState>(
		"connecting",
	);
	const retryDelayRef = useRef(INITIAL_RETRY_MS);
	const hadConnectionRef = useRef(false);

	useEffect(() => {
		if (!enabled) {
			setConnectionState("disconnected");
			return;
		}

		const controller = new AbortController();
		setConnectionState(
			hadConnectionRef.current ? "reconnecting" : "connecting",
		);

		void fetchEventSource(url, {
			signal: controller.signal,
			// authedFetch carries Authorization + inherits the two-budget
			// retry + spacebot:auth-exhausted observability (PR B).
			fetch: authedFetch,
			// Keep streaming when the tab is hidden so worker/chat updates
			// don't stall on inactive tabs.
			openWhenHidden: true,
			onopen: async (res) => {
				if (!res.ok) {
					throw new Error(`SSE open failed: ${res.status}`);
				}
				const wasReconnect = hadConnectionRef.current;
				hadConnectionRef.current = true;
				retryDelayRef.current = INITIAL_RETRY_MS;
				setConnectionState("connected");
				if (wasReconnect) {
					onReconnectRef.current?.();
				}
			},
			onmessage: (ev) => {
				// Server emits the event-type name in ev.event. Route onto
				// the existing handlers dictionary the same way the native
				// EventSource version did via addEventListener.
				const eventName = ev.event || "message";
				// `lagged` marks that the broadcast channel skipped events;
				// trigger resync via onReconnect.
				if (eventName === "lagged") {
					try {
						const data = JSON.parse(ev.data) as { skipped?: number };
						console.warn(`SSE lagged, skipped ${data.skipped} events`);
					} catch {
						console.warn("SSE lagged, skipped events");
					}
					onReconnectRef.current?.();
					return;
				}
				const handler = handlersRef.current[eventName];
				if (!handler) return;
				try {
					handler(JSON.parse(ev.data));
				} catch {
					handler(ev.data);
				}
			},
			onerror: (_err) => {
				setConnectionState("reconnecting");
				// Return the current backoff delay so fetch-event-source
				// waits this long before reconnecting. Advance for the next
				// failure. Throwing from onerror aborts; returning a number
				// keeps the retry loop alive.
				const delay = retryDelayRef.current;
				retryDelayRef.current = Math.min(
					delay * BACKOFF_MULTIPLIER,
					MAX_RETRY_MS,
				);
				return delay;
			},
			onclose: () => {
				// The library closes after onerror returns. Throw only if the
				// component has unmounted so the loop terminates cleanly.
				if (controller.signal.aborted) {
					throw new Error("aborted");
				}
			},
		}).catch((err: unknown) => {
			// AbortError lands here on unmount; everything else should have
			// been handled by onerror. Swallow quietly on abort.
			if (controller.signal.aborted) return;
			console.error("fetchEventSource terminated:", err);
		});

		return () => controller.abort();
	}, [url, enabled]);

	return { connectionState };
}
