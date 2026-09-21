<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { closeAllCleanEdits, hasDirtyEdit } from "../lib/editSessions";
  import { lang, sessions, settings, toast, toastErr } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Dropdown from "./Dropdown.svelte";

  let {
    onClose,
  }: {
    onClose: () => void;
  } = $props();

  let section = $state<"language" | "terminal" | "files" | "security" | "about">("language");
    let confirmClear: "hosts" | "sessions" | null = $state(null);
  let storageDir = $state("");
  let version = $state("");
  let hasHosts = $state(false);

  onMount(async () => {
    try {
      storageDir = await api.storagePath();
    } catch { /* ignore */ }
    try {
      version = await api.version();
    } catch { /* ignore */ }
    try {
      hasHosts = await api.hasKnownHosts();
    } catch { /* ignore */ }
    try {
      editors = await api.editorsList();
    } catch { /* keep Notepad-only fallback */ }
    await refreshEditTemp();
  });

  async function chooseDir() {
    try {
      const sel = await open({ directory: true });
      if (typeof sel === "string") {
        const next = { ...$settings, downloadDir: sel };
        await api.saveSettings(next);
        settings.set(next);
      }
    } catch (e) {
      toastErr(String(e));
    }
  }

  async function setLang(v: "auto" | "ru" | "en") {
    const next = { ...$settings, language: v };
    await api.saveSettings(next);
    settings.set(next);
  }

  async function setFontSize(v: number) {
    const n = Math.min(24, Math.max(9, Math.round(v) || 14));
    const next = { ...$settings, terminalFontSize: n };
    await api.saveSettings(next);
    settings.set(next);
  }

  async function setScrollback(v: number) {
    const n = Math.min(50000, Math.max(500, Math.round(v) || 5000));
    const next = { ...$settings, scrollback: n };
    await api.saveSettings(next);
    settings.set(next);
  }

  // View/Edit editor dropdown: only installed editors are offered;
  // a stale saved value (uninstalled since) falls back to Notepad.
  let editors = $state<{ id: string; name: string; available: boolean }[]>([]);
  let editorOptions = $derived.by(() => {
    const names: Record<string, string> = {
      notepad: tr($lang, "settings.editorNotepad"),
      "notepad++": tr($lang, "settings.editorNotepadPlus"),
      vscode: tr($lang, "settings.editorVscode"),
    };
    const avail = editors.filter((e) => e.available);
    const list = (avail.length ? avail : [{ id: "notepad", name: "Notepad", available: true }]).map((e) => ({
      value: e.id,
      label: names[e.id] ?? e.name,
    }));
    if (!list.some((o) => o.value === $settings.editor)) {
      queueMicrotask(() => setEditor(list[0].value as "notepad" | "notepad++" | "vscode", true));
    }
    return list;
  });

  async function setEditor(v: "notepad" | "notepad++" | "vscode", force = false) {
    if (!force && v === $settings.editor) return;
    const next = { ...$settings, editor: v };
    try {
      await api.saveSettings(next);
    } catch (e) {
      toastErr(String(e));
      return;
    }
    settings.set(next);
  }

  // View/Edit temp folder: active only when there is something to delete.
  // Blocked while any session holds unsaved edits (same `edit.pending`
  // wording as the dock chips). After clearing, clean chips are closed —
  // by construction no dirty session can exist at that point.
  let editTempFiles = $state(0);

  async function refreshEditTemp() {
    try {
      const st = await api.editTempStatus();
      editTempFiles = st.files;
    } catch {
      editTempFiles = 0;
    }
  }

  async function openEditTemp() {
    try {
      await api.editTempOpen();
    } catch (e) {
      toastErr(String(e));
    }
  }

  async function clearEditTemp() {
    let removed = 0;
    try {
      const r = await api.editTempClear();
      removed = r.removed;
    } catch (e) {
      toastErr(String(e));
      return;
    }
    closeAllCleanEdits();
    toast("ok", tr($lang, "settings.clearedEditTemp", { n: removed }));
    await refreshEditTemp();
  }

  async function doClear() {
    try {
      if (confirmClear === "hosts") {
        await api.clearHosts();
        hasHosts = false;
        toast("ok", tr($lang, "settings.clearedHosts"));
      } else if (confirmClear === "sessions") {
        await api.clearSessions();
        sessions.set([]);
        toast("ok", tr($lang, "settings.clearedSessions"));
      }
    } catch (e) {
      toastErr(String(e));
    } finally {
      confirmClear = null;
    }
  }

  const sections = [
    { id: "language", label: "settings.language" },
    { id: "terminal", label: "settings.terminal" },
    { id: "files", label: "settings.files" },
    { id: "security", label: "settings.security" },
    { id: "about", label: "settings.about" },
  ] as const;
</script>

<svelte:window
  onkeydown={(e) => {
    // Don't close settings while its nested confirm is open.
    if (e.key === "Escape" && !confirmClear) onClose();
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div
    class="modal modal-wide"
    in:scale={{ duration: 180, start: 0.96 }}
    out:fade={{ duration: 120 }}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{tr($lang, "settings.title")}</h2>
    <div class="set-body">
      <nav class="set-nav">
        {#each sections as s}
          <button
            class="set-nav-btn"
            class:active={section === s.id}
            onclick={() => (section = s.id)}
          >
            {tr($lang, s.label)}
          </button>
        {/each}
      </nav>
      <div class="set-content">
        {#if section === "language"}
          <div class="field">
            <label id="set-lang-label">{tr($lang, "settings.language")}</label>
            <Dropdown
              options={[
                { value: "auto", label: tr($lang, "settings.langAuto") },
                { value: "ru", label: tr($lang, "settings.langRu") },
                { value: "en", label: tr($lang, "settings.langEn") },
              ]}
              value={["auto", "ru", "en"].includes($settings.language) ? $settings.language : "auto"}
              onChange={(v) => setLang(v as "auto" | "ru" | "en")}
            />
          </div>
        {:else if section === "terminal"}
          <div class="field">
            <label for="set-font">{tr($lang, "settings.fontSize")}</label>
            <input
              id="set-font"
              type="number"
              min="9"
              max="24"
              value={$settings.terminalFontSize}
              onchange={(e) => setFontSize(Number((e.target as HTMLInputElement).value))}
              autocomplete="off"
            />
          </div>
          <div class="field">
            <label for="set-scroll">{tr($lang, "settings.scrollback")}</label>
            <input
              id="set-scroll"
              type="number"
              min="500"
              max="50000"
              step="500"
              value={$settings.scrollback}
              onchange={(e) => setScrollback(Number((e.target as HTMLInputElement).value))}
              autocomplete="off"
            />
          </div>
        {:else if section === "files"}
          <div class="field">
            <label for="set-dir">{tr($lang, "settings.downloadDir")}</label>
            <div class="field-row">
              <input id="set-dir" readonly value={$settings.downloadDir ?? tr($lang, "settings.notChosen")} autocomplete="off" />
              <button class="btn btn-sm" onclick={chooseDir}>{tr($lang, "settings.chooseDir")}</button>
            </div>
          </div>
          <div class="field">
            <label id="set-editor-label">{tr($lang, "settings.editor")}</label>
            <Dropdown
              options={editorOptions}
              value={$settings.editor}
              onChange={(v) => setEditor(v as "notepad" | "notepad++" | "vscode")}
            />
          </div>
          <div class="field">
            <label for="set-clear-edittemp">{tr($lang, "settings.editTemp")}</label>
            <div class="field-row">
              <button id="set-clear-edittemp" class="btn" disabled={$hasDirtyEdit || editTempFiles === 0} onclick={clearEditTemp}>
                {tr($lang, "settings.clearEditTemp")}
              </button>
              <button class="btn" onclick={openEditTemp}>
                {tr($lang, "settings.openEditTemp")}
              </button>
            </div>
            {#if $hasDirtyEdit}
              <span class="hint">{tr($lang, "edit.pending")}</span>
            {:else if editTempFiles === 0}
              <span class="hint">{tr($lang, "settings.noEditTemp")}</span>
            {/if}
          </div>
        {:else if section === "security"}
          <div class="field">
            <button class="btn" disabled={!hasHosts} onclick={() => (confirmClear = "hosts")}>
              {tr($lang, "settings.clearHosts")}
            </button>
            {#if !hasHosts}
              <span class="hint">{tr($lang, "settings.noHosts")}</span>
            {/if}
          </div>
          <div class="field">
            <button
              class="btn btn-danger"
              disabled={$sessions.length === 0}
              onclick={() => (confirmClear = "sessions")}
            >
              {tr($lang, "settings.clearSessions")}
            </button>
            {#if $sessions.length === 0}
              <span class="hint">{tr($lang, "settings.noSessions")}</span>
            {/if}
          </div>
          <div class="field">
            <label>{tr($lang, "settings.storagePath")}</label>
            <div class="storage">{storageDir}</div>
          </div>
        {:else}
          <div class="about">
            <div class="about-name">Termix <span class="ver">{tr($lang, "settings.version")} {version}</span></div>
            <p>{tr($lang, "settings.aboutText")}</p>
          </div>
        {/if}
      </div>
    </div>
    <div class="modal-actions">
      <button class="btn" onclick={onClose}>{tr($lang, "settings.close")}</button>
    </div>
  </div>
</div>

{#if confirmClear}
  <ConfirmDialog
    title={confirmClear === "hosts" ? tr($lang, "settings.clearHosts") : tr($lang, "settings.clearSessions")}
    body=""
    confirmText={tr($lang, "common.ok")}
    onConfirm={doClear}
    onCancel={() => (confirmClear = null)}
  />
{/if}

<style>
  .modal-wide {
    width: 620px;
  }
  .set-body {
    display: flex;
    gap: 20px;
    min-height: 260px;
  }
  .set-nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 170px;
  }
  .set-nav-btn {
    text-align: left;
    padding: 9px 12px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--text-sub);
    font-size: 13px;
    cursor: pointer;
  }
  .set-nav-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .set-nav-btn.active {
    background: var(--accent-dim);
    color: var(--accent);
  }
  .set-content {
    flex: 1;
    min-width: 0;
  }
  .storage {
    font-family: Consolas, monospace;
    font-size: 12px;
    color: var(--text-sub);
    word-break: break-all;
  }
  .about-name {
    font-size: 18px;
    font-weight: 700;
  }
  .ver {
    font-size: 12px;
    color: var(--text-faint);
    font-weight: 400;
  }
  .about p {
    color: var(--text-sub);
    font-size: 13px;
    line-height: 1.6;
  }
  .about .sub {
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
