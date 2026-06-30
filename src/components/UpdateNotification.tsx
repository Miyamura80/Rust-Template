import { useAppUpdate } from "../hooks/useAppUpdate";

export function UpdateNotification() {
	const {
		status,
		info,
		progress,
		error,
		updateNow,
		remindLater,
		skipVersion,
		retry,
	} = useAppUpdate();

	if (status === "idle" || !info) return null;

	return (
		<div className="update-banner">
			<div className="update-banner-content">
				{status === "available" && (
					<>
						<div className="update-banner-text">
							<strong>v{info.version} is available</strong>
							{info.body && (
								<span className="update-banner-changelog">
									{info.body.length > 120
										? `${info.body.slice(0, 120)}...`
										: info.body}
								</span>
							)}
						</div>
						<div className="update-banner-actions">
							<button
								type="button"
								className="update-btn update-btn--primary"
								onClick={updateNow}
							>
								Update Now
							</button>
							<button
								type="button"
								className="update-btn update-btn--secondary"
								onClick={remindLater}
							>
								Later
							</button>
							<button
								type="button"
								className="update-btn update-btn--secondary"
								onClick={skipVersion}
							>
								Skip This Version
							</button>
						</div>
					</>
				)}

				{status === "downloading" && (
					<>
						<div className="update-banner-text">
							<strong>Downloading v{info.version}...</strong>
							{progress >= 0 && (
								<span className="update-banner-progress-label">
									{progress}%
								</span>
							)}
						</div>
						<div className="update-banner-progress">
							<div
								className={`update-banner-progress-bar${progress < 0 ? " update-banner-progress-bar--indeterminate" : ""}`}
								style={progress >= 0 ? { width: `${progress}%` } : undefined}
							/>
						</div>
					</>
				)}

				{status === "ready" && (
					<div className="update-banner-text">
						<strong>Update installed. Restarting...</strong>
					</div>
				)}

				{status === "error" && (
					<>
						<div className="update-banner-text">
							<strong>Update failed</strong>
							<span className="update-banner-error">{error}</span>
						</div>
						<div className="update-banner-actions">
							<button
								type="button"
								className="update-btn update-btn--primary"
								onClick={retry}
							>
								Retry
							</button>
							<button
								type="button"
								className="update-btn update-btn--secondary"
								onClick={remindLater}
							>
								Dismiss
							</button>
						</div>
					</>
				)}
			</div>
		</div>
	);
}
