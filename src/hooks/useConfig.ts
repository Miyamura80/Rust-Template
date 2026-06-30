import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

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
		invoke<AppConfig>("get_app_config")
			.then((data) => {
				setConfig(data);
				setLoading(false);
			})
			.catch((err) => {
				setError(err.toString());
				setLoading(false);
			});
	}, []);

	return { config, loading, error };
}
