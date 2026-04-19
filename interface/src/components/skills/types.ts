import type {SkillInfo, RegistrySkill} from "@spacebot/api-client/client";

export type SkillView = "directory" | "bundled" | "create";

export type SelectedSkill =
	| {type: "installed"; skill: SkillInfo}
	| {type: "registry"; skill: RegistrySkill};
