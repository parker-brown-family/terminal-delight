<!-- briefing: {"id":"reader-feedback-markdown","medium":"markdown","purpose":"engineering","reader":"a person reviewing an agent's work","minutes":2,"response":"correction","evidence":"code-trace"} -->

# A Comment's Return

You leave a comment on an explanation, then the agent revises that explanation. Before acting on the comment, the agent needs to establish which passage and version you were reading.

<a id="sec-target"></a>
## The Target

Each section keeps a stable identifier when its heading changes. The feedback map carries that target, the reader's words and the document's content fingerprint. The target locates the passage; the fingerprint identifies the version. [Mechanism](evidence/anchors.md#mechanism)

<a id="sec-return"></a>
## The Return

The agent checks the map against the supplied document. A matching identity, version and target establishes where the comment belongs. That does not mean the concern has been answered.

| What changed? | What the agent does |
|---|---|
| Nothing | Read the identified passage and answer the comment |
| The document's content | Review the old and current context |
| The target disappeared | Keep the comment unresolved |
| The version is unknown | State the uncertainty and review the context |

The reader's text stays in every result. [Validation and source](evidence/anchors.md#validation)

<a id="sec-use"></a>
## Using It

This Markdown example uses the portable [saved feedback map](engineering.feedback.json). Add a comment there, then supply both files to the agent or copy the map while naming this document. Native inline commenting depends on your reader. The [HTML rendition](engineering.html) has its own comment controls.

This is an engineering example of this skill's mechanism. Video annotation is reserved for later. [Limits](evidence/anchors.md#limits)
