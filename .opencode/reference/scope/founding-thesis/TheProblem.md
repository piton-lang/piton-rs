# The Problem

## Description

As potent as the wow-factor is when you prompt an agent with a minimal description of what you want and it builds it, that process has left me feeling like a lot is missing – not strictly from the result, but more from my involvement in the process and my ability to precisely control the outcome. “Great job, brother Claude. Now make X work like Y.” It does, but also ends up making C into a Q. So you prompt again, and prompt again, again. Eventually you’re almost certain to arrive on something close enough to correct, and sometimes this is absolutely fine. If you’re building a small, one-off tool then this is nothing short of incredible.
But, I would argue that for a sufficiently complex system, something more robust and traceable is needed, something that is an artifact of the process itself, not just the output of the process. I believe this becomes even more relevant as more people work on an agentic codebase; the prompts that led to an outcome live within the session, and at best are a faint memory in the one mind of the one developer. Despite how it may seem, neither agents nor humans can read minds.
Many of the agentic platforms provide mechanisms like AGENTS.md and CLAUDE.md that provide a level of instruction to the agent. Further still, more granular skills and agents are written in Markdown and committed to the repository. Now you can write durable instructions that the agents will reference when building the code.
The problem I started to encounter is that in the aim of describing interconnected systems, pure prose is inefficient and prone to decay. Any author is likely to inadvertently describe subsystems and instructions with phrasing that is at best ambiguous enough to cause conflict. With traditional programming, we have all these wonderful tools to promote reuse and avoid duplication. Markdown doesn’t have these tools.
At the same time, pure prose is an incredible way to say
```
“the button should be blue with contrasting text.”
```
Compare that to
```css
.button {
  background-color: #00a;
  color: #fff;
}
```
Which one would you rather write?
It’s this combination of structure and prose both as first-class citizens that Piton attempts to solve. So with that, let’s take a look at Piton.
