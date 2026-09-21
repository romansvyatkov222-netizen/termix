<script lang="ts">
  import { ArrowLeft, House, RotateCw, Search, Upload } from "lucide-svelte";
  import { api } from "../lib/api";
  import { tr, type Lang } from "../lib/i18n";
  import { displayPath, resolveInputPath } from "../lib/format";

  // Extracted verbatim from FilesPanel (phase 3). Behaviour unchanged.
  let {
    lang,
    pathInput = $bindable("~/"),
    search = $bindable(""),
    homeDir,
    onHome,
    onUp,
    onGo,
    onNavigate,
    onBrowseUpload,
    onRefresh,
  }: {
    lang: Lang;
    pathInput?: string;
    search?: string;
    homeDir: string | null;
    onHome: () => void;
    onUp: () => void;
    onGo: () => void;
    onNavigate: (path: string) => void;
    onBrowseUpload: () => void;
    onRefresh: () => void;
  } = $props();

  // Path autocomplete: typing `dir/` or `dir/par` suggests subfolders of `dir`.
  let suggestOpen = $state(false);
  let suggestItems = $state<string[]>([]);
  let suggestIdx = $state(0);
  let suggestReq = 0;
  let suggestTimer: ReturnType<typeof setTimeout> | null = null;
  let typed = false;
  let blurTimer: ReturnType<typeof setTimeout> | null = null;

  function closeSuggest() {
    suggestOpen = false;
    if (suggestTimer) {
      clearTimeout(suggestTimer);
      suggestTimer = null;
    }
  }

  async function updateSuggest() {
    const raw = resolveInputPath(pathInput);
    const my = ++suggestReq;
    if (!raw.startsWith("/") || raw.length < 2) {
      suggestOpen = false;
      return;
    }
    const slash = raw.lastIndexOf("/");
    const dir = raw.slice(0, slash) || "/";
    const prefix = raw.slice(slash + 1).toLowerCase();
    let list;
    try {
      list = await api.list(dir);
    } catch {
      if (my === suggestReq) suggestOpen = false;
      return;
    }
    if (my !== suggestReq) return;
    const dirs = list
      .filter((e) => e.isDir && e.name.toLowerCase().startsWith(prefix))
      .map((e) => displayPath(dir === "/" ? `/${e.name}` : `${dir}/${e.name}`))
      .slice(0, 8);
    if (dirs.length === 1 && dirs[0].toLowerCase() === raw.toLowerCase()) {
      suggestOpen = false;
      return;
    }
    suggestItems = dirs;
    suggestIdx = 0;
    suggestOpen = dirs.length > 0;
  }

  function scheduleSuggest() {
    typed = true;
    if (suggestTimer) clearTimeout(suggestTimer);
    suggestTimer = setTimeout(() => {
      suggestTimer = null;
      void updateSuggest();
    }, 150);
  }

  // Navigation from elsewhere (dblclick, home, …) rewrites the input:
  // drop a stale dropdown, but not the one just opened by typing.
  let lastInput = pathInput;
  $effect(() => {
    if (pathInput !== lastInput) {
      lastInput = pathInput;
      if (typed) {
        typed = false;
      } else {
        suggestOpen = false;
      }
    }
  });

  function acceptSuggest(item: string, navigate: boolean) {
    if (blurTimer) {
      clearTimeout(blurTimer);
      blurTimer = null;
    }
    if (navigate) {
      closeSuggest();
      onNavigate(resolveInputPath(item));
    } else {
      typed = true;
      pathInput = item.endsWith("/") ? item : `${item}/`;
      lastInput = pathInput;
      void updateSuggest();
    }
  }

  function onPathKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" && suggestOpen && suggestItems.length) {
      e.preventDefault();
      suggestIdx = (suggestIdx + 1) % suggestItems.length;
    } else if (e.key === "ArrowUp" && suggestOpen && suggestItems.length) {
      e.preventDefault();
      suggestIdx = (suggestIdx - 1 + suggestItems.length) % suggestItems.length;
    } else if (e.key === "Enter") {
      if (suggestOpen && suggestItems.length) acceptSuggest(suggestItems[suggestIdx], true);
      else onGo();
    } else if (e.key === "Tab" && suggestOpen && suggestItems.length) {
      e.preventDefault();
      acceptSuggest(suggestItems[suggestIdx], false);
    } else if (e.key === "Escape") {
      closeSuggest();
    }
  }

  function onPathBlur() {
    // Let a suggestion click (mousedown) land before closing.
    if (blurTimer) clearTimeout(blurTimer);
    blurTimer = setTimeout(() => {
      blurTimer = null;
      suggestOpen = false;
    }, 150);
  }

  function onPathFocus() {
    const raw = resolveInputPath(pathInput);
    if (raw.startsWith("/") && raw.length > 1) scheduleSuggest();
  }
</script>

<div class="toolbar">
  <button
    class="icon-btn"
    title={homeDir ? `${tr(lang, "files.home")} (${homeDir})` : tr(lang, "files.home")}
    onclick={onHome}
  >
    <House size={18} />
  </button>
  <button class="icon-btn" title={tr(lang, "files.back")} onclick={onUp}><ArrowLeft size={18} /></button>
  <div class="path-wrap">
    <input
      class="path"
      bind:value={pathInput}
      oninput={scheduleSuggest}
      onkeydown={onPathKey}
      onblur={onPathBlur}
      onfocus={onPathFocus}
      spellcheck="false"
      autocomplete="off"
    />
    {#if suggestOpen && suggestItems.length}
      <div class="suggest" role="listbox">
        {#each suggestItems as item, i (item)}
          <button
            class="ctx-item"
            class:sel={i === suggestIdx}
            role="option"
            aria-selected={i === suggestIdx}
            onmousedown={(e) => {
              e.preventDefault();
              acceptSuggest(item, true);
            }}
            onmousemove={() => (suggestIdx = i)}
          >
            📁 {item}
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <button class="btn btn-sm" onclick={onGo}>{tr(lang, "files.go")}</button>
  <div class="search-wrap">
    <span class="search-icon" aria-hidden="true"><Search size={14} /></span>
    <input class="search" placeholder={tr(lang, "files.search")} bind:value={search} autocomplete="off" />
  </div>
  <button class="btn btn-sm" onclick={onBrowseUpload}><Upload size={14} /> {tr(lang, "files.upload")}</button>
  <button class="icon-btn" title={tr(lang, "files.refresh")} onclick={onRefresh}><RotateCw size={18} /></button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border-soft);
  }
  .path-wrap {
    position: relative;
    flex: 1;
    display: flex;
    min-width: 120px;
  }
  .path {
    flex: 1;
    padding: 7px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--text);
    font-family: Consolas, monospace;
    font-size: 12px;
    outline: none;
    min-width: 120px;
  }
  .path:focus {
    border-color: var(--border-focus);
  }
  .suggest {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    z-index: 210;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 5px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    max-height: 240px;
    overflow-y: auto;
  }
  .suggest .ctx-item {
    font-family: Consolas, monospace;
    font-size: 12px;
  }
  .suggest .ctx-item.sel {
    background: var(--bg-hover);
  }
  .search {
    width: 200px;
    padding: 7px 12px 7px 30px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--text);
    font-size: 12px;
    outline: none;
  }
  .search-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }
  .search-icon {
    position: absolute;
    left: 10px;
    display: inline-flex;
    color: var(--text-faint);
    pointer-events: none;
  }
</style>
