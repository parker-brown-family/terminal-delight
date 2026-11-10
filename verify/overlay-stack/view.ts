// The words a person reads on the overlay-stack chart (pstack /statechart).
const word: Record<string, string> = {
  composer: "composer",
  palette: "design menu",
  book: "Settings",
  help: "keys sheet",
};
const Z: Record<string, number> = { composer: 1, palette: 2, book: 3, help: 4 };
export const title = "What is on top, and what your keys reach";
export const state = (v: any) => {
  const open: string[] = [...v.open].sort((a: string, b: string) => Z[a] - Z[b]);
  if (open.length === 0) return "Just the panes";
  return open.map((s) => word[s]).join(" under ");
};
const chord: Record<string, string> = {
  book: "you press Ctrl+,",
  help: "you press F1",
  composer: "you press Ctrl+Shift+E",
  palette: "you press Ctrl+Shift+D",
};
export const step = (action: string, _from: any, to: any) =>
  action === "Raise" || action === "PressCtrlComma"
    ? chord[to.opened]
    : action === "TypeLetter"
      ? "you type a letter"
      : action === "PressEsc"
        ? "you press Esc"
        : action;
