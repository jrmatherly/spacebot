import { useState, useCallback, useEffect, useRef, lazy, Suspense } from "react";
import { Button, Input } from "@spacedrive/primitives";
import { useServer } from "@/hooks/useServer";
import {
	dragRegionAttributes,
	IS_DESKTOP,
	spawnBundledProcess,
} from "@/platform";

const Orb = lazy(() => import("@/components/Orb"));

type SidecarState = "idle" | "starting" | "running" | "error";

// Maximum time to wait for the daemon's "HTTP server listening" line before
// giving up and transitioning to the error state. Cold startup on a fresh
// machine can include migrations, lance index hydration, and the embedding
// model download, which together take ~15-30s; 45s is a safe upper bound.
const STARTUP_TIMEOUT_MS = 45_000;

// Short timeout for the pre-spawn health probe. If something is already
// listening on the local port we short-circuit; if nothing is there the probe
// fails fast and we fall through to the normal spawn path.
const HEALTH_PROBE_TIMEOUT_MS = 500;

const LOCAL_SERVER_URL = "http://localhost:19898";

async function probeLocalServer(): Promise<boolean> {
	try {
		const response = await fetch(`${LOCAL_SERVER_URL}/api/health`, {
			signal: AbortSignal.timeout(HEALTH_PROBE_TIMEOUT_MS),
		});
		return response.ok;
	} catch {
		return false;
	}
}

/**
 * Full-screen connection screen shown when the app cannot reach
 * the spacebot server. Allows changing the server URL and, in
 * desktop hosts with a bundled binary, starting a local instance.
 */
export function ConnectionScreen() {
	const { serverUrl, setServerUrl, state, hasBundledServer } = useServer();
	const [draft, setDraft] = useState(serverUrl);
	const [sidecarState, setSidecarState] = useState<SidecarState>("idle");
	const [sidecarError, setSidecarError] = useState<string | null>(null);
	const startupTimerRef = useRef<number | null>(null);

	// Keep draft in sync when serverUrl changes externally
	useEffect(() => {
		setDraft(serverUrl);
	}, [serverUrl]);

	// Clear any pending startup timer if the component unmounts mid-spawn.
	useEffect(() => {
		return () => {
			if (startupTimerRef.current !== null) {
				window.clearTimeout(startupTimerRef.current);
				startupTimerRef.current = null;
			}
		};
	}, []);

	const clearStartupTimer = useCallback(() => {
		if (startupTimerRef.current !== null) {
			window.clearTimeout(startupTimerRef.current);
			startupTimerRef.current = null;
		}
	}, []);

	const handleConnect = useCallback(() => {
		setServerUrl(draft);
	}, [draft, setServerUrl]);

	const handleKeyDown = useCallback(
		(event: React.KeyboardEvent) => {
			if (event.key === "Enter") handleConnect();
		},
		[handleConnect],
	);

	const handleStartLocal = useCallback(async () => {
		if (!IS_DESKTOP) return;
		setSidecarState("starting");
		setSidecarError(null);

		// If something is already listening on the local port (e.g. a manually
		// launched daemon or a leftover from a previous desktop session), skip
		// the spawn and just connect. This also turns the common EADDRINUSE
		// failure into a success path.
		if (await probeLocalServer()) {
			setSidecarState("running");
			setServerUrl(LOCAL_SERVER_URL);
			return;
		}

		try {
			let sawReady = false;
			const spawned = await spawnBundledProcess("binaries/spacebot-daemon", [
				"start",
				"--foreground",
			], {
				onError: (error) => {
					clearStartupTimer();
					setSidecarState("error");
					setSidecarError(error);
				},
				onClose: (data) => {
					clearStartupTimer();
					if (!sawReady || data.code === null || data.code !== 0) {
						setSidecarState("error");
						setSidecarError(
							data.code === null
								? "Process exited before the HTTP server became ready"
								: `Process exited with code ${data.code}`,
						);
						return;
					}
					setSidecarState("idle");
				},
				onStdout: (line) => {
					// Look for the "HTTP server listening" log line
					if (line.includes("HTTP server listening")) {
						sawReady = true;
						clearStartupTimer();
						setSidecarState("running");
						// Point the app at localhost
						setServerUrl(LOCAL_SERVER_URL);
					}
				},
			});

			if (!spawned) {
				setSidecarState("error");
				setSidecarError("Bundled server is unavailable in this host.");
				return;
			}

			// Arm the startup timeout. onStdout-match / onClose / onError will
			// clear it; if none of those fire within the budget we transition
			// to error so the button does not sit stuck on "Starting...".
			clearStartupTimer();
			startupTimerRef.current = window.setTimeout(() => {
				startupTimerRef.current = null;
				setSidecarState((current) =>
					current === "starting" ? "error" : current,
				);
				setSidecarError(
					"Timed out waiting for the local server to start. Check logs at ~/.spacebot/logs/",
				);
			}, STARTUP_TIMEOUT_MS);

			setSidecarState("starting");
		} catch (error) {
			clearStartupTimer();
			setSidecarState("error");
			setSidecarError(
				error instanceof Error ? error.message : String(error),
			);
		}
	}, [setServerUrl, clearStartupTimer]);

	const isChecking = state === "checking";

	return (
		<div className="flex h-screen w-full flex-col items-center justify-center bg-app overflow-hidden">
			{/* Draggable titlebar region for the desktop host */}
			{IS_DESKTOP && (
				<div
					{...dragRegionAttributes()}
					className="fixed inset-x-0 top-0 h-8"
				/>
			)}

			<div className="flex w-full max-w-md flex-col items-center gap-8 px-6">
				{/* Orb + Title */}
				<div className="flex flex-col items-center gap-3">
					<div className="relative h-[160px] w-[160px]">
						<div className="absolute inset-[calc(5%-10px)] z-0">
							<img
								src="/ball.png"
								alt="Spacebot"
								className="h-full w-full object-contain"
							/>
						</div>
						<div className="absolute inset-0 z-10">
							<Suspense fallback={null}>
								<Orb
									hue={-30}
									hoverIntensity={0}
									rotateOnHover
								/>
							</Suspense>
						</div>
					</div>
					<h1 className="font-plex text-xl font-semibold text-ink">
						Connect to Spacebot
					</h1>
					<p className="text-center text-sm text-ink-dull">
						Enter the URL of a running Spacebot instance, or start
						one locally.
					</p>
				</div>

				{/* URL Input */}
				<div className="flex w-full flex-col gap-3">
					<label className="text-xs font-medium text-ink-dull">
						Server URL
					</label>
					<div className="flex gap-2">
						<Input
							value={draft}
							onChange={(event) => setDraft(event.target.value)}
							onKeyDown={handleKeyDown}
							placeholder="http://localhost:19898"
							className="flex-1"
							size="md"
							disabled={isChecking}
						/>
						<Button
							onClick={handleConnect}
							disabled={isChecking || !draft.trim()}
							size="md"
							variant="accent"
							className="bg-[hsl(282,70%,57%)] text-white shadow hover:bg-[hsl(282,70%,50%)] hover:text-white"
						>
							Connect
						</Button>
					</div>

					{/* Connection status */}
					{isChecking ? (
						<p className="text-xs text-ink-faint">
							Connecting...
						</p>
					) : state === "disconnected" ? (
						<p className="text-xs text-ink-faint">
							Not connected
						</p>
					) : null}
				</div>

				{/* Divider */}
				{hasBundledServer && (
					<>
						<div className="flex w-full items-center gap-3">
							<div className="h-px flex-1 bg-app-line" />
							<span className="text-xs text-ink-faint">or</span>
							<div className="h-px flex-1 bg-app-line" />
						</div>

						{/* Start Local Server */}
						<div className="flex w-full flex-col gap-3">
						<Button
							onClick={handleStartLocal}
							variant="outline"
							disabled={
								sidecarState === "starting" ||
									sidecarState === "running"
								}
								className="w-full"
							>
								{sidecarState === "starting"
									? "Starting Spacebot..."
									: sidecarState === "running"
										? "Server Running"
										: "Start Local Server"}
							</Button>

							{sidecarState === "starting" && (
								<p className="text-xs text-ink-faint">
									Starting the bundled Spacebot binary. This
									may take a few seconds on first run...
								</p>
							)}

							{sidecarState === "error" && sidecarError && (
								<p className="text-xs text-red-400">
									{sidecarError}
								</p>
							)}
						</div>
					</>
				)}

				{/* Footer hint */}
				<p className="text-center text-xs text-ink-faint">
					Spacebot runs on port 19898 by default.
					{!hasBundledServer && (
						<>
							{" "}
							Install via{" "}
							<span className="font-mono text-ink-dull">
								docker
							</span>{" "}
							or download from{" "}
							<a
								href="https://spacebot.sh"
								target="_blank"
								rel="noopener noreferrer"
								className="text-accent hover:underline"
							>
								spacebot.sh
							</a>
						</>
					)}
				</p>
			</div>
		</div>
	);
}
