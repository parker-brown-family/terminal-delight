// The words a person reads on the open-document chart (pstack /statechart).
export const title = "An agent opening its document beside itself";
export const state = (v: any) =>
  (v.square === "person" ? "Your document is open over the pane" : v.square === "agent" ? "The agent's document is open over its pane" : "Nothing over the pane") + (v.rail ? ", and one waits on the rail" : "");
export const group = (v: any) => (v.square === "person" ? "Yours" : v.square === "agent" ? "Agent's" : "Clear");
export const colors = { Clear: "teal", Yours: "yellow", "Agent's": "blue" };
export const step = (a: string, f: any, t: any) =>
  ({ PersonOpens: "you open a document", PersonCloses: "you close it", AgentOpensBesideOrSide: null, AgentIsRefused: null, PersonOpensFromTheRail: "you open it from the rail" } as any)[a] ??
  (a === "AgentOpensHere" ? (t.last === "held" ? "the agent opens one: held, onto the rail" : "the agent opens one here") : a);
