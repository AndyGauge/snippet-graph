---
title: "Attributions"
lead: "Nothing here is new on its own. Each idea below has a long history, and these are the systems and papers it came from."
description: "The version control systems, merge tools and code-generation research that snippet-graph builds on, with links."
---

snippet-graph combines four ideas: treat code as small named units instead of files, identify each version by its content, understand code by its syntax tree, and let tests decide what to retry. Each was done well before, mostly for people rather than for models. Where this project differs, it says so. Where it only borrows a name or a shape, it says that too.

## Version control that worked on code, not text

**[ENVY/Developer](https://paulhammant.com/2017/09/01/smalltalk-envy/) and VisualAge.** Smalltalk's ENVY kept classes and methods as individually versioned elements in a repository instead of files, and replaced check-out and check-in with ownership of components and editions. IBM's VisualAge for Java carried it into Java; its repository held program elements rather than files. This is the closest ancestor of treating one function as the unit of history. See also [Mastering ENVY/Developer](https://www.hoepli.it/amp/mastering-envydeveloper/9780521666503.html).

**[Monticello](https://news.squeak.org/category/tools/monticello) (Squeak Smalltalk).** Package snapshots with branching and merging, and in its second version tracking of individual classes and methods, so that arbitrary snippets of code could be versioned.

**[Unison](https://www.unison-lang.org/docs/the-big-idea/).** Each definition is identified by a hash of its syntax tree, and names are separate metadata pointing at hashes. Code lives in a structured codebase, not in text files, so a rename never breaks a caller. snippet-graph's stored versions are content-addressed in the same spirit, but it keeps ordinary Rust files as the working copy and uses hashes for history, not for the language.

**[Git](https://github.com/progit/progit/blob/master/en/09-git-internals/01-chapter9.markdown).** Objects named by the hash of their content, trees that point at blobs, and commits that point at trees. snippet-graph's store follows the same Merkle-DAG shape, with a function body as the leaf instead of a file.

**[Darcs and Pijul](https://pijul.org/manual/why_pijul).** Version control built on changes that commute: two changes either apply in either order or one depends on the other. That is the same question snippet-graph asks of functions, whether one needs the other or not, and it is why independent functions can be filled in parallel. snippet-graph does not implement patch theory; the link is the dependency question, not the algebra. The formal background is in [Jacobson's paper on Darcs](https://ww3.math.ucla.edu/camreport/cam09-83.pdf).

## Tools that understand code structure when merging and diffing

**[SemanticMerge](https://docs.plasticscm.com/semanticmerge/intro-guide/semanticmerge-intro-guide) and Plastic SCM's [semantic method history](https://blog.plasticscm.com/2013/10/new-semantic-methodhist-is-now-out.html).** Merging on code trees instead of lines, so moving a method is not a conflict, and tracking a method's history across renames and moves. See the [InfoQ report](https://www.infoq.com/news/2013/04/Semantic-Merge).

**[GumTree](https://github.com/gumtreediff/gumtree).** Syntax-aware differencing that finds moved and renamed elements as well as inserted and deleted ones. See [Fine-grained, accurate and scalable source differencing](https://conf.researchr.org/details/icse-2024/icse-2024-research-track/148/Fine-grained-accurate-and-scalable-source-differencing) for the line of work.

**[Spork](https://arxiv.org/pdf/2202.05329) and [Mergiraf](https://lwn.net/Articles/1042355).** Structured merge for Java, then a tree-sitter-based merge driver for many languages. Both treat the syntax tree as the thing being merged.

## Graphs of code

**[Code property graphs](https://mlsec.tu-berlin.de/docs/2014-ieeesp.pdf)** (Yamaguchi, Golde, Arp, Rieck). Syntax trees, control flow and data flow joined in one graph that is queried by traversal. snippet-graph's graph is much simpler: nodes for definitions, edges for calls and uses, found from the syntax tree alone with no type checking, as the [tour](../tour/) explains.

**[Aider's repository map](https://github.com/Cryect/RepoMapper).** Tree-sitter finds definitions and references, a graph of them is ranked by personalised PageRank against the conversation, and the top of it is fitted into a token budget. It uses the graph to choose what a model reads; snippet-graph also uses it to choose what order to write in. The link is to a port of the idea; check Aider's own documentation for its exact current behaviour.

## Generating code with tests and decomposition

**[Parsel](https://arxiv.org/abs/2212.10561)** (Zelikman, Huang, Poesia, Goodman, Haber). Break a task into a hierarchy of natural-language function descriptions, build a dependency graph, and search over implementations using tests. This is the nearest published relative of snippet-graph's approach, and it came first.

**[CodeT](https://arxiv.org/abs/2207.10397)** (Chen et al.). A model writes tests for its own candidate programs, and candidates are chosen by how they fare on them.

**[AlphaCodium](https://arxiv.org/abs/2401.08500).** A multi-stage flow that runs and repairs generated code against public tests and tests the model generated, and finds that the generated tests close blind spots.

**[Self-Debugging](https://arxiv.org/abs/2304.05128) and [Reflexion](https://arxiv.org/pdf/2303.11366).** Feeding execution results back to the model so it can correct its own program, and keeping a written reflection on failures for the next attempt. The failure ledger here is a plain, computed cousin of that memory.

## What is different here

Putting these together has a few specifics: the unit of repair is the one function a failing test blames, a function that exhausts its attempt budget is rolled back to the last version that compiled, and the result is a handoff report written for a person who must finish the work. None of the above is claimed as invented here. If you know of earlier work that belongs on this page, [open an issue](https://github.com/andygauge/snippet-graph/issues).
