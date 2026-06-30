import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";

type UpdateStatus = "idle" | "available" | "downloading" | "ready" | "error";

interface UpdateInfo {
	version: string;
	body: string | undefined;
}

interface AppUpdateState {
	status: UpdateStatus;
	info: UpdateInfo | null;
	progress: number;
	error: string | null;
	updateNow: () => void;
	remindLater: () => void;
	skipVersion: () => void;
	retry: () => void;
}

const SKIPPED_VERSION_KEY = "tauri-app:skipped-update-version";

export function useAppUpdate(): AppUpdateState {
	const [status, setStatus] = useState<UpdateStatus>("idle");
	const [info, setInfo] = useState<UpdateInfo | null>(null);
	const [progress, setProgress] = useState(0);
	const [error, setError] = useState<string | null>(null);
	const [dismissed, setDismissed] = useState(false);
	const updateRef = useRef<Awaited<ReturnType<typeof check>> | null>(null);

	const checkForUpdate = useCallback(async () => {
		try {
			const update = await check();
			if (!update) return;

			const skipped = localStorage.getItem(SKIPPED_VERSION_KEY);
			if (skipped === update.version) return;

			updateRef.current = update;
			setInfo({ version: update.version, body: update.body });
			setStatus("available");
			setDismissed(false);
			setError(null);
		} catch (e) {
			// Silently ignore check failures on startup
			console.warn("Update check failed:", e);
		}
	}, []);

	useEffect(() => {
		const timer = setTimeout(checkForUpdate, 3000);
		return () => clearTimeout(timer);
	}, [checkForUpdate]);

	const updateNow = useCallback(async () => {
		const update = updateRef.current;
		if (!update) return;

		setStatus("downloading");
		setProgress(0);
		setError(null);

		try {
			let totalBytes = 0;
			let downloadedBytes = 0;
			await update.downloadAndInstall((event) => {
				if (event.event === "Started" && event.data.contentLength) {
					totalBytes = event.data.contentLength;
				} else if (event.event === "Progress") {
					downloadedBytes += event.data.chunkLength;
					if (totalBytes > 0) {
						setProgress(Math.round((downloadedBytes / totalBytes) * 100));
					} else {
						setProgress(-1);
					}
				} else if (event.event === "Finished") {
					setProgress(100);
				}
			});
			setStatus("ready");
			await relaunch();
		} catch (e) {
			setError(e instanceof Error ? e.message : String(e));
			setStatus("error");
		}
	}, []);

	const remindLater = useCallback(() => {
		setDismissed(true);
	}, []);

	const skipVersion = useCallback(() => {
		if (info) {
			localStorage.setItem(SKIPPED_VERSION_KEY, info.version);
		}
		setDismissed(true);
	}, [info]);

	const retry = useCallback(async () => {
		await checkForUpdate();
		if (updateRef.current) {
			await updateNow();
		}
	}, [checkForUpdate, updateNow]);

	const effectiveStatus = dismissed ? "idle" : status;

	return {
		status: effectiveStatus,
		info,
		progress,
		error,
		updateNow,
		remindLater,
		skipVersion,
		retry,
	};
}
