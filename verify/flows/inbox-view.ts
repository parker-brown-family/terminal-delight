// The INBOX flow in the words of its shelves (pstack /statechart).
export const title = "One agent through the INBOX shelves";
const shelfWord: Record<string, string> = {
  LookAtMe: "LOOK AT ME",
  Working: "WORKING",
  Snoozed: "SNOOZED",
  Quiet: "SLEEP & QUIET",
  Done: "DONE",
};
const shelf = (v: any) =>
  v.mark === "done" ? "Done"
  : v.mark === "snoozed" ? "Snoozed"
  : v.agent === "asks" || v.agent === "finished" || v.woke ? "LookAtMe"
  : v.agent === "working" ? "Working"
  : "Quiet";
const agentWord: Record<string, string> = {
  working: "working",
  asks: "asking you something",
  finished: "finished",
  idle: "idle",
};
export const state = (v: any) => {
  const s = shelf(v);
  const why = s === "LookAtMe" && v.woke && v.agent !== "asks" && v.agent !== "finished" ? "back from a snooze" : agentWord[v.agent];
  return `${shelfWord[s]}: ${why}`;
};
export const group = (v: any) => shelf(v);
export const colors = { LookAtMe: "orange", Working: "teal", Snoozed: "purple", Quiet: "blue", Done: "tan" };
export const step = (action: string, from: any) =>
  ({
    AgentAsks: from.mark === "snoozed" ? "it asks something new; the snooze ends" : from.mark === "done" ? "it asks something new; done lifts" : "it asks you something",
    AgentFinishes: from.mark === "done" ? "it finishes again; done lifts" : "it finishes",
    YouAnswer: "you answer",
    YouLook: "you look at it",
    YouSendWork: "you send it work",
    YouSnooze: "you snooze it",
    SnoozeEnds: "the snooze's hour comes",
    YouMarkDone: "you mark it done",
    YouReopen: "you reopen it",
  } as any)[action];
