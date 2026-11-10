// The Settings book's flow in the reader's words (pstack /statechart).
export const title = "Moving through the Settings book";
export const state = (v: any) => {
  if (v.book === "closed") return "Settings closed";
  if (v.host && v.panel) return "A panel open on the page";
  if (v.searching) return "Searching";
  return "A page of rows";
};
export const group = (v: any) => (v.book === "closed" ? "Closed" : v.host && v.panel ? "Panel" : "Book");
export const colors = { Closed: "tan", Book: "teal", Panel: "blue" };
export const step = (action: string, from: any, to: any) =>
  ({
    YouOpen: "you press Ctrl+, or the gear",
    YouType: "you type",
    YouPickAResult: "you press Enter on a result",
    YouPressOpen: "you press Open",
    YouClickBesideThePanel: "you click beside the panel",
    YouPickAnotherPage: "you pick another page",
    YouPressEsc:
      from.host && from.panel ? "Esc: back to the rows" : from.searching ? "Esc: the search clears" : "Esc: the book closes",
  } as any)[action];
