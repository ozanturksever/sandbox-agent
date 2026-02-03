export type InstallCommandBlock = {
	label: string;
	commands: string[];
};

export type NonExecutableBinaryMessageOptions = {
	binPath: string;
	trustPackages: string;
	bunInstallBlocks: InstallCommandBlock[];
	genericInstallCommands?: string[];
};

export function isBunRuntime(): boolean {
  if (typeof process?.versions?.bun === "string") return true;
  const userAgent = process?.env?.npm_config_user_agent || "";
  return userAgent.includes("bun/");
}

const PERMISSION_ERRORS = new Set(["EACCES", "EPERM", "ENOEXEC"]);

export function isPermissionError(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const code = (error as { code?: unknown }).code;
  return typeof code === "string" && PERMISSION_ERRORS.has(code);
}

export function formatNonExecutableBinaryMessage(
	options: NonExecutableBinaryMessageOptions,
): string {
	const { binPath, trustPackages, bunInstallBlocks, genericInstallCommands } =
		options;

	const lines = [`sandbox-agent binary is not executable: ${binPath}`];

	if (isBunRuntime()) {
		lines.push(
			"Allow Bun to run postinstall scripts for native binaries and reinstall:",
		);
		for (const block of bunInstallBlocks) {
			lines.push(`${block.label}:`);
			for (const command of block.commands) {
				lines.push(`  ${command}`);
			}
		}
		lines.push(`Or run: chmod +x "${binPath}"`);
		return lines.join("\n");
	}

	lines.push(
		"Postinstall scripts for native packages did not run, so the binary was left non-executable.",
	);
	if (genericInstallCommands && genericInstallCommands.length > 0) {
		lines.push("Reinstall with scripts enabled:");
		for (const command of genericInstallCommands) {
			lines.push(`  ${command}`);
		}
	} else {
		lines.push("Reinstall with scripts enabled for:");
		lines.push(`  ${trustPackages}`);
	}
	lines.push(`Or run: chmod +x "${binPath}"`);
	return lines.join("\n");
}
