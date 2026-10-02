export const restarts = { count: 0 };

export async function relaunch() {
  restarts.count += 1;
}

export async function exit() {}
