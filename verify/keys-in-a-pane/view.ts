// The words a person reads on the keys-in-a-pane chart (pstack /statechart).
export const title = "One pane, two places to type";
export const state = (v: any) =>
  v.sticky && v.rename ? "A note and a name both being written" : v.sticky ? "Writing a sticky note" : v.rename ? "Renaming the pane" : "Typing to the shell";
export const group = (v: any) => (v.sticky && v.rename ? "Clash" : v.sticky || v.rename ? "Writing" : "Shell");
export const colors = { Shell: "teal", Writing: "yellow", Clash: "tan" };
export const step = (action: string) =>
  ({ YouPressAltS: "you press Alt+S", YouOpenTheRenameBox: "you double-click the title", YouTypeALetter: null, YouFinishTheNote: "you finish the note", YouFinishTheName: "you finish the name" } as any)[action] ?? action;
