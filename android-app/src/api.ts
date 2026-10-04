import { invoke } from "@tauri-apps/api/core";

export interface PairingInfo {
  ip: string;
  port: number;
  token: string;
  key_b64: string;
}

export interface MobileStatus {
  hasMaster: boolean;
  unlocked: boolean;
  paired: boolean;
  pairing: PairingInfo | null;
  entryCount: number;
  /** 设备支持生物识别 */
  bioAvailable: boolean;
  /** 锁定状态可用指纹解锁（有缓存会话） */
  bioUsable: boolean;
  /** 已设置密保（可找回主密码） */
  hasSecurity: boolean;
}

export interface EntryListItem {
  id: string;
  type: string;
  name: string;
  username: string;
  url: string;
  category: string;
  favorite: boolean;
  updatedAt: number;
  passwordMasked: string;
}

export interface TrashItemInfo {
  id: string;
  type: string;
  name: string;
  username: string;
  category: string;
  deletedAt: number;
  passwordMasked: string;
}

export interface CategoryInfo {
  name: string;
  icon: string;
  parentName: string | null;
}

export interface EntryDetail {
  id: string;
  type: string;
  name: string;
  username: string;
  email: string;
  phone: string;
  nickname: string;
  url: string;
  notes: string;
  category: string;
  favorite: boolean;
  updatedAt: number;
}

/** 新增/编辑条目的完整字段（密码仅在保存瞬间提交） */
export interface EntryDraft {
  id: string;
  type: string;
  name: string;
  username: string;
  email: string;
  phone: string;
  password: string;
  nickname: string;
  url: string;
  notes: string;
  category: string;
  tags: string[];
  favorite: boolean;
}

export interface HealthInfo {
  running: boolean;
  port: number;
  vaultUnlocked: boolean;
  pairedCount: number;
  ip: string | null;
}

export const api = {
  mobileStatus: () => invoke<MobileStatus>("mobile_status"),
  mobileSetup: (password: string, confirm: string) =>
    invoke<void>("mobile_setup", { password, confirm }),
  mobileUnlock: (password: string) => invoke<void>("mobile_unlock", { password }),
  mobileLock: () => invoke<void>("mobile_lock"),
  mobileLockAll: () => invoke<void>("mobile_lock_all"),
  mobileBiometricAvailable: () => invoke<boolean>("mobile_biometric_available"),
  mobileBiometricUnlock: () => invoke<void>("mobile_biometric_unlock"),
  mobileTestBiometric: () => invoke<void>("mobile_test_biometric"),
  mobileShareText: (text: string, subject: string) =>
    invoke<void>("mobile_share_text", { text, subject }),
  mobileInstallApk: (path: string) => invoke<string>("mobile_install_apk", { path }),
  mobileOpenUrl: (url: string) => invoke<string>("mobile_open_url", { url }),
  mobileExit: () => invoke<void>("mobile_exit"),
  mobileCheckUpdate: () =>
    invoke<{ version: string; currentVersion: string } | null>("mobile_check_update"),
  mobileDownloadUpdate: () => invoke<string>("mobile_download_update"),
  mobileOpenBiometricSettings: () =>
    invoke<string>("mobile_open_biometric_settings"),
  mobileSecurityStatus: () => invoke<boolean>("mobile_security_status"),
  mobileUpdateSecurity: (args: {
    currentPassword: string;
    newPassword?: string;
    newConfirm?: string;
    questions?: string[];
    answers?: string[];
  }) => invoke<string | null>("mobile_update_security", args),
  mobileRecoveryReset: (args: {
    recoveryCode: string;
    answers: string[];
    newPassword: string;
    confirm: string;
  }) => invoke<void>("mobile_recovery_reset", args),
  mobilePcRecoveryStatus: () =>
    invoke<{ available: boolean; questions: string[] | null }>("mobile_pc_recovery_status"),
  mobilePcRecoveryReset: (args: {
    recoveryCode: string;
    answers: string[];
    newPassword: string;
    confirm: string;
  }) => invoke<void>("mobile_pc_recovery_reset", args),
  mobileExportBackup: () => invoke<string>("mobile_export_backup"),
  mobileImportBackup: (text: string, password: string) =>
    invoke<number>("mobile_import_backup", { text, password }),
  mobileChangeMaster: (oldPassword: string, newPassword: string, confirm: string) =>
    invoke<void>("mobile_change_master", { oldPassword, newPassword, confirm }),
  syncHealth: (ip: string, port: number) =>
    invoke<HealthInfo>("sync_health", { ip, port }),
  syncConnect: (qrPayload: string) =>
    invoke<PairingInfo>("sync_connect", { qrPayload }),
  syncPull: () => invoke<number>("sync_pull"),
  syncPush: () => invoke<number>("sync_push"),
  syncDisconnect: () => invoke<void>("sync_disconnect"),
  mobileList: () => invoke<EntryListItem[]>("mobile_list"),
  mobileEntryPassword: (entryId: string) =>
    invoke<string>("mobile_entry_password", { entryId }),
  mobileEntryDetail: (entryId: string) =>
    invoke<EntryDetail>("mobile_entry_detail", { entryId }),
  mobileSaveEntry: (entry: EntryDraft) =>
    invoke<EntryListItem[]>("mobile_save_entry", { entry }),
  mobileDeleteEntry: (entryId: string) => invoke<void>("mobile_delete_entry", { entryId }),
  mobileTrashList: () => invoke<TrashItemInfo[]>("mobile_trash_list"),
  mobileRestoreEntry: (entryId: string) =>
    invoke<EntryListItem[]>("mobile_restore_entry", { entryId }),
  mobileTrashPurge: (entryId: string) => invoke<void>("mobile_trash_purge", { entryId }),
  mobileTrashClear: () => invoke<void>("mobile_trash_clear"),
  mobileCategoryList: () => invoke<CategoryInfo[]>("mobile_category_list"),
  mobileCategoryCreate: (name: string, icon: string) =>
    invoke<CategoryInfo[]>("mobile_category_create", { name, icon }),
  mobileCategoryRename: (oldName: string, newName: string, icon: string) =>
    invoke<CategoryInfo[]>("mobile_category_rename", { oldName, newName, icon }),
  mobileCategoryDelete: (name: string) =>
    invoke<CategoryInfo[]>("mobile_category_delete", { name }),
  mobileGeneratePassword: (length?: number) =>
    invoke<string>("mobile_generate_password", { length }),
};
