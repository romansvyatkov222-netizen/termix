<script lang="ts">
  import { Eye } from "@lucide/svelte";
  import { fade, scale } from "svelte/transition";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { connectFlow } from "../lib/connect";
  import { lang, sessions, toast, toastErr } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import type { Session } from "../lib/types";
  import Dropdown from "./Dropdown.svelte";
  import Tip from "./Tip.svelte";

  let {
    editing,
    onClose,
    onConnected,
  }: {
    editing?: Session | null;
    onClose: () => void;
    onConnected: () => void;
  } = $props();

  let name = $state(editing?.name ?? "");
  let host = $state(editing?.host ?? "");
  let port = $state(editing?.port ?? 22);
  let username = $state(editing?.username ?? "");
  let authType = $state<"password" | "privateKey">(editing?.authType ?? "password");
  // Secrets are input-only: empty input keeps the stored secret.
  let password = $state("");
  let keyPath = $state("");
  let passphrase = $state("");
  let showPassword = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  function errText(code: string | null | undefined): string {
    if (!code) return "";
    const key = `err.${code}`;
    const t = tr($lang, key);
    return t === key ? code : t;
  }

  async function browseKey() {
    try {
      const sel = await open({ multiple: false, title: "key" });
      if (typeof sel === "string") keyPath = sel;
    } catch {
      try {
        const p = await api.pickDir();
        void p;
      } catch {
      }
    }
  }

  function collect(): Session {
    const cleanUser = username.replace(/[А-Яа-яЁё]/g, "");
    if (cleanUser !== username) username = cleanUser;
    const pw = password.trim();
    const kp = keyPath.trim();
    return {
      id: editing?.id ?? "",
      name: name.trim(),
      host: host.trim(),
      port: Number(port) || 22,
      username: username.trim(),
      authType,
      password: authType === "password" ? (pw ? password : null) : null,
      keyPath: authType === "privateKey" ? (kp || null) : null,
      keyPassphrase: null,
      lastUsedAt: editing?.lastUsedAt ?? null,
    };
  }

  async function saveOnly(): Promise<Session | null> {
    error = null;
    const s = collect();
    try {
      if (editing) {
        const upd = await api.updateSession(s);
        sessions.update((l) => l.map((x) => (x.id === upd.id ? upd : x)));
        return upd;
      } else {
        const created = await api.createSession(s);
        sessions.update((l) => [created, ...l]);
        return created;
      }
    } catch (e) {
      error = errText(String(e));
      return null;
    }
  }

  async function doConnect() {
    if (busy) return;
    busy = true;
    error = null;
    const isNew = !editing;
    const snapshot: Session | null = editing ? { ...editing } : null;
    try {
      const saved = await saveOnly();
      if (!saved) {
        busy = false;
        return;
      }
      const secrets =
        authType === "password"
          ? password
            ? { password }
            : {}
          : keyPath.trim()
            ? { keyPath: keyPath.trim(), passphrase }
            : {};
      const ok = await connectFlow(saved.id, secrets);
      if (ok) {
        const list = await api.listSessions();
        sessions.set(list);
        onConnected();
      } else if (isNew) {
        await api.deleteSession(saved.id).catch(() => {});
        sessions.set(await api.listSessions().catch(() => []));
      } else if (snapshot) {
        const restored = await api.updateSession(snapshot).catch(() => null);
        if (restored) {
          sessions.update((l) => l.map((x) => (x.id === restored.id ? restored : x)));
        } else {
          sessions.set(await api.listSessions().catch(() => []));
        }
      }
    } catch (e) {
      error = String(e);
      toastErr(String(e));
    } finally {
      busy = false;
    }
  }

  async function doSave() {
    if (busy) return;
    busy = true;
    try {
      const saved = await saveOnly();
      if (saved) {
        toast("ok", tr($lang, "common.save"));
        onClose();
      }
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && !busy) onClose();
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div
    class="modal"
    in:scale={{ duration: 180, start: 0.96 }}
    out:fade={{ duration: 120 }}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{editing ? tr($lang, "connect.titleEdit") : tr($lang, "connect.titleNew")}</h2>
    {#if error}
      <div class="form-error">{error}</div>
    {/if}
    <div class="field">
      <label for="c-name">{tr($lang, "connect.name")}</label>
      <input id="c-name" bind:value={name} placeholder="my-server" autocomplete="off" />
    </div>
    <div class="grid2">
      <div class="field">
        <label for="c-host">{tr($lang, "connect.host")}</label>
        <input id="c-host" bind:value={host} placeholder="example.com" autocomplete="off" />
      </div>
      <div class="field">
        <label for="c-port">{tr($lang, "connect.port")}</label>
        <input id="c-port" type="number" bind:value={port} min="1" max="65535" autocomplete="off" />
      </div>
    </div>
    <div class="field">
      <label for="c-user">{tr($lang, "connect.username")}</label>
      <input
        id="c-user"
        bind:value={username}
        placeholder="root"
        autocomplete="off"
        oninput={() => {
          const clean = username.replace(/[А-Яа-яЁё]/g, "");
          if (clean !== username) username = clean;
        }}
      />
    </div>
    <div class="field">
      <label id="c-auth-label">{tr($lang, "connect.authType")}</label>
      <Dropdown
        options={[
          { value: "password", label: tr($lang, "connect.authPassword") },
          { value: "privateKey", label: tr($lang, "connect.authKey") },
        ]}
        value={authType}
        onChange={(v) => (authType = v as "password" | "privateKey")}
      />
    </div>
    {#if authType === "password"}
      <div class="field">
        <label for="c-pass">{tr($lang, "connect.password")}</label>
        <div class="field-row">
          <input
            id="c-pass"
            type={showPassword ? "text" : "password"}
            bind:value={password}
            placeholder={editing?.hasStoredSecret ? "••••••••" : tr($lang, "connect.passwordPlaceholder")}
            autocomplete="new-password"
          />
          <Tip tip={showPassword ? tr($lang, "connect.hidePassword") : tr($lang, "connect.showPassword")}>
            <button
              class="btn btn-sm eye-btn"
              class:on={showPassword}
              onclick={() => (showPassword = !showPassword)}
            >
              <Eye size={16} />
            </button>
          </Tip>
        </div>
        {#if editing?.hasStoredSecret}
          <span class="hint">{tr($lang, "connect.savedHint")}</span>
        {/if}
      </div>
    {:else}
      <div class="field">
        <label for="c-key">{tr($lang, "connect.keyPath")}</label>
        <div class="field-row">
          <input
            id="c-key"
            bind:value={keyPath}
            placeholder={editing?.hasStoredSecret ? "••••••••" : "C:\\Users\\you\\.ssh\\id_rsa"}
            autocomplete="off"
          />
          <button class="btn btn-sm" onclick={browseKey}>{tr($lang, "connect.browse")}</button>
        </div>
        {#if editing?.hasStoredSecret}
          <span class="hint">{tr($lang, "connect.savedHint")}</span>
        {/if}
      </div>
      <div class="field">
        <label for="c-phrase">{tr($lang, "connect.passphrase")}</label>
        <input id="c-phrase" type="password" bind:value={passphrase} placeholder={tr($lang, "connect.passphrasePlaceholder")} autocomplete="new-password" />
        <span class="hint">{tr($lang, "connect.passphraseHint")}</span>
      </div>
    {/if}
    <div class="modal-actions">
      <button class="btn" onclick={onClose} disabled={busy}>{tr($lang, "connect.cancel")}</button>
      {#if editing}
        <button class="btn" onclick={doSave} disabled={busy}>{tr($lang, "connect.save")}</button>
      {/if}
      <button class="btn btn-accent-soft" onclick={doConnect} disabled={busy}>
        {busy ? tr($lang, "connect.connecting") : tr($lang, "connect.connect")}
      </button>
    </div>
  </div>
</div>

<style>
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 110px;
    gap: 12px;
  }
  .eye-btn {
    color: var(--text-faint);
    line-height: 0;
  }
  .eye-btn.on {
    color: var(--accent);
  }
  .eye-btn.on :global(svg) {
    filter: drop-shadow(0 0 5px rgba(94, 234, 212, 0.9));
  }
</style>
