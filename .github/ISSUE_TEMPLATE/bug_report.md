---
name: Bug report
about: Help reproduce and fix a SolidCraft problem
title: ''
labels: ''
assignees: ''
---

## Behavior

What did you expect, and what happened instead?

## Reproduction

Include steps, SolidCraft version/commit and relevant environment details. Prefer a small example without private design data.

When the problem shows on a particular design, `solidcraft-cli script design.solidcraft --out repro.json` turns its history into a script that `solidcraft-cli eval repro.json` replays; attach it, trimmed to the failing step if you can.

## Screenshots of the behavior (when observable)

For observable CAD, UI or model behavior, include actual descriptive screenshots or a short recording from a small reproduction. This includes kernel, geometry, assembly and backend effects that can be shown in the app. Coding agents should run the app and capture the evidence themselves, rather than asking a human for evidence they can produce. Show the actual failure/result, preferably before and after, with brief steps, expected/actual behavior and annotations where helpful.

Flowcharts and Mermaid diagrams cannot substitute for evidence of visible behavior. Use diagrams for architectural reasoning when actual screenshots cannot illustrate the point. Changes with no useful observable behavior may omit screenshot/video evidence. Exclude private designs and sensitive details; attach media or link to an accessible recording rather than committing generated review artifacts.
