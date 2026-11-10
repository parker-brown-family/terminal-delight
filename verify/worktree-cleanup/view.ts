// The words a person reads on the worktree-cleanup chart (pstack /statechart).
export const title = "Removing one worktree while others may walk into it";

const where = (v: any) =>
  v.inside ? (v.dirty ? "someone inside, with changes" : "someone inside") : v.dirty ? "changes, nobody inside" : "clean, nobody inside";

export const state = (v: any) => {
  switch (v.ui) {
    case "idle":
      return "Not listed";
    case "listed":
      return "Listed to remove";
    case "armed":
      return "Remove pressed once";
    case "moved":
      return "Moved out of reach";
    case "rechecked":
      return v.inside ? "Asking git, someone inside" : "Asking git";
    case "gitChecked":
      return v.inside ? (v.dirty ? "git deleting, someone inside writing" : "git deleting, someone inside") : "git deleting";
    case "gone":
      return v.harm === "deletedWork"
        ? "Removed, with uncommitted work in it"
        : v.harm === "deletedUnderAProcess"
          ? "Removed under a running process"
          : "Removed";
  }
  return v.ui;
};

export const group = (v: any) =>
  v.ui === "gone" ? "Gone" : v.ui === "idle" ? "Kept" : "Review";

export const colors = { Kept: "teal", Review: "yellow", Gone: "tan" };

export const step = (action: string, from: any, to: any) => {
  switch (action) {
    case "Enter":
      return "a shell or agent walks in";
    case "Leave":
      return to.ui === "gone" || from.ui === "idle" || from.ui === "listed" || from.ui === "armed" ? null : "it walks out";
    case "Write":
      return "it writes a file";
    case "Look":
      return to.ui === "listed" ? "you open the review: it qualifies" : "you open the review: it is kept";
    case "Arm":
      return "you press Remove";
    case "Move":
      return "you press again: it is moved out of reach";
    case "Recheck":
      return to.ui === "idle" ? "you press again: someone is inside, kept" : "you press again";
    case "GitCheck":
      return to.ui === "idle" ? "git refuses: it has changes" : "git finds it clean";
    case "GitDelete":
      return "git deletes it";
    case "Gone":
      return null;
  }
  return action;
};
