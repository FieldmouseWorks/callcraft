# 30-Day Pilot — Design Partner Center

Status: draft. Purpose: prove or kill the wedge in 30 days with one real center,
one training manager, and five trainees. If the training officer can't author a
scenario in under an hour or the scoring doesn't agree with human grading, we
learn that cheaply and stop.

## Preconditions (week 0)

**Permission and framing.** This is a personal open-source project, built on
personal time and equipment, with a partner communications center as a design
partner. Confirm the
outside-employment/IP policy in writing before any agency content is touched.
What we ask for: a short note from the training manager acknowledging the pilot
and its scope.

**Content boundary.**
- Partner SOPs and any agency-specific scenarios stay in a private local pack.
  Nothing agency-owned is committed to a public repo.
- Public artifacts use synthetic people, addresses, and numbers.
- No real call audio, transcripts, CAD data, or CJI enters the tool at any point.
- The partner center's name and logo are not used publicly without written
  approval.

**Hosting answer before code.** Decide who runs the pilot instance: a laptop in
the training room, a partner-approved VM, or a state/regional host. The
trainer must not have to run Docker.

## Week 1 — Format review and synthetic dry run

- One-hour review with the training manager and two trainers: the one-page
  format, the worked example, and the rubric model.
- Pick three call types they consider high-value and low-frequency. Candidates
  to propose: adult chest pain, structure fire report, welfare check with a
  language barrier.
- Author synthetic versions of all three before the meeting ends or in the
  following two days. Target: under 60 minutes per scenario by a trainer, not
  an engineer.
- Text-mode dry runs: the trainers type as the caller and the trainee; the
  rubric scores from the transcript. No AI in the loop yet. This validates the
  authoring model independently of voice quality.

## Week 2 — Agency-tailored authoring

- In a private pack, the trainers adapt the three scenarios to the partner's
  SOPs:
  the required-question checklists, the pre-arrival instruction references, the
  local geography references.
- The authoring tool lints every file; the schema is the gatekeeper. Fixes are
  made in the tool, not by hand-editing YAML unless the trainer prefers it.
- Measure: authoring time per scenario, number of lint errors, how many times
  a trainer asks "what does this field mean?" If either number is high, the
  format is wrong and we revise it before voice work.

## Week 3 — Voice and blind grading

- Voice adapter online, English first. Measure per-session cost (STT + model +
  TTS) and latency (p50/p95). Targets: under $1 per session, under 1.5 seconds
  round trip.
- Five trainees run each scenario once, by voice.
- The training officer grades the transcripts blind — rubric items only, no
  machine results visible — and we compare machine vs. human per item.
- Drift counter must read zero: no hallucinated addresses, names, medications,
  or times across all sessions. Any rejection is logged and reviewed.

## Week 4 — Decision

Success criteria, all required:

1. **Authoring:** three scenarios authored by trainers from their own SOPs, each
   in under 60 minutes, with no engineer present.
2. **Scoring:** machine checklist results agree with blind human grading on at
   least 85% of items; every critical item agrees on every session.
3. **Portability:** scenarios export from one instance, import into a second,
   and diff readably in git.
4. **Cost and latency:** measured and published; under $1 per session and under
   1.5 seconds p50 round trip.
5. **Trust:** the training officer asks to keep using it in week five, unprompted.

Failure is informative: if authoring needs an engineer, the wedge is fiction;
if grading disagrees, scoring ships as human-only and the format still stands.

## What to show at the first meeting

1. This plan, one page.
2. The format spec, one page.
3. One worked scenario file, printed, with the rubric highlighted.
4. A two-minute text-mode run, live or recorded.
5. The success criteria above, so the training manager knows what "pilot" means.

## Decisions the partner center needs to make

- Is this a personal project with a design-partner arrangement, or a partner
  project? (Affects IP, procurement, and who can speak about it.)
- Who reviews content before trainees see it?
- Are transcripts retained? For how long? (Default: transcripts only, no audio
  stored, retained for the pilot duration unless the center says otherwise.)
- Who hosts the pilot instance?

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Employment/IP dispute | Written scope note in week 0; no agency time, equipment, or content in public artifacts |
| SOP confidentiality | Private local pack; no public commit; content review before trainee use |
| "Free but unsupported" procurement wall | Decide host and support contact before week 1; state 911 board or consortium as backup |
| ASR errors poison scoring | Anchor-based checks; human review on critical items; text mode for authoring validation |
| Trainee flagged unfairly | Human override always available and recorded; tool framed as practice, never certification |
| Caller drift kills trust | Closed fact base; drift rejections logged; zero-drift criterion in week 3 |
| Pilot becomes a product requirement | Written success criteria; stop at week 4 and decide deliberately |
