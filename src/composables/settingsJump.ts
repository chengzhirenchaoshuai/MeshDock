import { inject, type InjectionKey } from "vue";

/** 可以直接跳到的设置卡片 */
export type SettingsTarget = "home" | "relay" | "interval" | "about";

export const settingsJumpKey: InjectionKey<(target: SettingsTarget) => void> = Symbol("settingsJump");

/** 取得“跳到设置某张卡片”的函数，由 App 提供 */
export const useSettingsJump = () => inject(settingsJumpKey, () => {});
