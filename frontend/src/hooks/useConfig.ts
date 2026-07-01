import { useEffect, useState } from "react";
import { ApiError, fetchConfig } from "../api/client";

export interface AppConfig {
	model_name: string;
	dot_global_config_health_check: boolean;
	dev_env: string;
	example_parent: {
		example_child: string;
	};
	default_llm: {
		default_model: string;
		fallback_model?: string;
		default_temperature: number;
		default_max_tokens: number;
	};
	llm_config: {
		cache_enabled: boolean;
		retry: {
			max_attempts: number;
			min_wait_seconds: number;
			max_wait_seconds: number;
		};
	};
	features: Record<string, boolean>;
}

export function useConfig() {
	const [config, setConfig] = useState<AppConfig | null>(null);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		fetchConfig<AppConfig>()
			.then((data) => {
				setConfig(data);
				setLoading(false);
			})
			.catch((err) => {
				// ApiError carries the server's error code (e.g. INTERNAL_ERROR).
				setError(
					err instanceof ApiError
						? `${err.code}: ${err.message}`
						: err instanceof Error
							? err.message
							: String(err),
				);
				setLoading(false);
			});
	}, []);

	return { config, loading, error };
}
