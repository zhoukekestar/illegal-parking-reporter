import { onMounted, onUnmounted } from "vue";

export interface ReviewShortcutHandlers {
  prev: () => void;
  next: () => void;
  accept: () => void;
  reject: () => void;
  defer: () => void;
  togglePlay: () => void;
  undo: () => void;
}

/**
 * 审核键盘快捷键 (DEVELOPMENT_PLAN.md §三 §五 P4)
 *   ← / →    切换上一/下一条
 *   ↑        采纳 (Accepted)
 *   ↓        丢弃 (Rejected)
 *   D        待定 (Deferred)
 *   Space    播放 / 暂停
 *   U / Cmd+Z 撤销最近一次操作
 *
 * 输入框获得焦点时 (input/textarea/contenteditable) 不拦截
 */
export function useReviewShortcuts(handlers: ReviewShortcutHandlers) {
  function isEditable(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return true;
    return target.isContentEditable;
  }

  function onKeydown(e: KeyboardEvent) {
    if (isEditable(e.target)) return;

    switch (e.key) {
      case "ArrowLeft":
        e.preventDefault();
        handlers.prev();
        break;
      case "ArrowRight":
        e.preventDefault();
        handlers.next();
        break;
      case "ArrowUp":
        e.preventDefault();
        handlers.accept();
        break;
      case "ArrowDown":
        e.preventDefault();
        handlers.reject();
        break;
      case " ":
        e.preventDefault();
        handlers.togglePlay();
        break;
      case "d":
      case "D":
        e.preventDefault();
        handlers.defer();
        break;
      case "u":
      case "U":
        e.preventDefault();
        handlers.undo();
        break;
      case "z":
      case "Z":
        if (e.metaKey || e.ctrlKey) {
          e.preventDefault();
          handlers.undo();
        }
        break;
    }
  }

  onMounted(() => {
    window.addEventListener("keydown", onKeydown);
  });
  onUnmounted(() => {
    window.removeEventListener("keydown", onKeydown);
  });
}
