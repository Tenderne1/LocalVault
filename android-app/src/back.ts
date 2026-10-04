// Android 返回键桥：MainActivity.onBackPressed → evaluateJavascript 调 window.__lvHandleBack
// 各页面通过 registerBackHandler 注册自己的返回处理（返回 true = 已消费，不再冒泡）；
// 处理顺序：后注册的先处理（页面级优先），最后是 App 层的兜底（两次返回退出）。

export type BackHandler = () => boolean;

const handlers: BackHandler[] = [];

/** 注册返回处理，返回取消注册函数 */
export function registerBackHandler(h: BackHandler): () => void {
  handlers.push(h);
  return () => {
    const i = handlers.indexOf(h);
    if (i >= 0) handlers.splice(i, 1);
  };
}

/** 依次让 handler 处理返回键，返回是否有 handler 消费了本次返回 */
export function dispatchBack(): boolean {
  for (let i = handlers.length - 1; i >= 0; i--) {
    try {
      if (handlers[i]()) return true;
    } catch {
      /* 单个 handler 异常不阻断其他 */
    }
  }
  return false;
}
