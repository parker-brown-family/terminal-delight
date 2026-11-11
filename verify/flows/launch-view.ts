// The words a person reads on the launch chart (pstack /statechart).
export const title = "Launching an agent from the composer";
export const state = (v: any) => {
  if (v.page === "away") return v.last === "front" ? "In the agent's new tab" : "Composer put away";
  if (v.last === "refused") return "Composer up, saying why it did not start";
  if (v.last === "behind") return "Composer up and empty: it started behind";
  return v.sentence === "written" ? (v.problem ? "Composer up, a choice it will refuse" : "Composer up, ready to send") : "Composer up, nothing written";
};
export const group = (v: any) => (v.page === "away" ? "Away" : v.last === "refused" ? "Refused" : "Composing");
export const colors = { Away: "teal", Composing: "yellow", Refused: "tan" };
export const step = (a: string, f: any, t: any) =>
  ({ YouOpenIt: "you open the composer", YouWrite: "you write", YouChooseSomething: "you change a chip", YouPutItAway: "you put it away" } as any)[a] ??
  (a === "YouSend" ? (t.last === "refused" ? "you send: refused" : t.last === "behind" ? "you press Ctrl+Enter" : "you press Enter") : a);
