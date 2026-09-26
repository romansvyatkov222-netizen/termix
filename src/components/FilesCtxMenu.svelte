<script lang="ts">
  import { tick } from "svelte";
  import { Download, FileArchive, FilePlusCorner, FolderPlus, NotebookPen, Pencil, RotateCw, Trash, Upload } from "@lucide/svelte";
  import { tr, type Lang } from "../lib/i18n";

  let {
    lang,
    x,
    y,
    hasTarget,
    canDownload,
    canArchive,
    canEdit,
    onDownload,
    onArchive,
    onEdit,
    onRename,
    onDelete,
    onNewFile,
    onNewFolder,
    onUploadHere,
    onRefresh,
  }: {
    lang: Lang;
    x: number;
    y: number;
    hasTarget: boolean;
    canDownload: boolean;
    canArchive: boolean;
    canEdit: boolean;
    onDownload: () => void;
    onArchive: () => void;
    onEdit: () => void;
    onRename: () => void;
    onDelete: () => void;
    onNewFile: () => void;
    onNewFolder: () => void;
    onUploadHere: () => void;
    onRefresh: () => void;
  } = $props();

  let menuEl: HTMLDivElement | null = $state(null);
  let pos = $state({ x: 0, y: 0 });
  $effect(() => {
    const ix = x;
    const iy = y;
    pos = { x: ix, y: iy };
    void tick().then(() => {
      if (!menuEl) return;
      const r = menuEl.getBoundingClientRect();
      let nx = ix;
      let ny = iy;
      if (r.bottom > window.innerHeight - 8) ny = Math.max(8, iy - r.height);
      if (r.right > window.innerWidth - 8) nx = Math.max(8, ix - r.width);
      if (nx !== pos.x || ny !== pos.y) pos = { x: nx, y: ny };
    });
  });
</script>

<div class="ctx-menu" bind:this={menuEl} style="left:{pos.x}px; top:{pos.y}px" onclick={(e) => e.stopPropagation()}>
  {#if hasTarget}
    {#if canDownload}
      <button class="ctx-item" onclick={onDownload}>
        <Download size={14} /> {tr(lang, "files.ctxDownload")}
      </button>
    {/if}
    {#if canArchive}
      <button class="ctx-item" onclick={onArchive}>
        <FileArchive size={14} /> {tr(lang, "files.ctxDownloadArchive")}
      </button>
    {/if}
    {#if canEdit}
      <button class="ctx-item" onclick={onEdit}>
        <NotebookPen size={14} /> {tr(lang, "files.ctxEdit")}
      </button>
    {/if}
    <button class="ctx-item" onclick={onRename}>
      <Pencil size={14} /> {tr(lang, "files.ctxRename")}
    </button>
    <button class="ctx-item danger" onclick={onDelete}>
      <Trash size={14} /> {tr(lang, "files.ctxDelete")}
    </button>
    <div class="ctx-sep"></div>
  {/if}
  <button class="ctx-item" onclick={onNewFile}>
    <FilePlusCorner size={14} /> {tr(lang, "files.ctxNewFile")}
  </button>
  <button class="ctx-item" onclick={onNewFolder}>
    <FolderPlus size={14} /> {tr(lang, "files.ctxNewFolder")}
  </button>
  <button class="ctx-item" onclick={onUploadHere}>
    <Upload size={14} /> {tr(lang, "files.ctxUploadHere")}
  </button>
  <button class="ctx-item" onclick={onRefresh}><RotateCw size={14} /> {tr(lang, "files.ctxRefresh")}</button>
</div>
