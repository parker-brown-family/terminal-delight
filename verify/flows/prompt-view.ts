// The words a person reads on the prompt chart (pstack /statechart).
export const title = "One agent prompting another across the Floor (first prompt)";
export const state = (v: any) => {
  const s = v.stage[1];
  return ({ none: "Not sent", refused: "Refused: no line at prompt", waiting: "Waiting for the worker to rest", typing: "Pasted, Enter not pressed",
    delivered: "Entered, not yet seen working", working: "The worker is working on it", finished: "Finished", failed: "Failed, and says where its text may be" } as any)[s] ?? s;
};
export const group = (v: any) => (["finished"].includes(v.stage[1]) ? "Done" : ["refused", "failed"].includes(v.stage[1]) ? "Stopped" : "Moving");
export const colors = { Moving: "yellow", Done: "teal", Stopped: "tan" };
export const step = (a: string) =>
  ({ World: null, Send: "the agent calls prompt_agent", TypeIt: "the worker rests: it is pasted", LineErased: "the line is erased", PasteShows: "the paste shows on the worker's screen",
    PressEnter: "Enter, once", NoProof: "no proof the paste landed", WorkerTakesIt: "the worker starts", WorkerRests: "the worker rests again" } as any)[a] ?? a;
