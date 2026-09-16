export function getCurrentWindow() {
  return {
    minimize: async () => {},
    toggleMaximize: async () => {},
    isMaximized: async () => false,
    close: async () => {},
    startResizeDragging: async () => {},
    startDragging: async () => {},
    onResized: async (_cb: () => void) => () => {}
  };
}
