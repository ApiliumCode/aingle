# Deep Context - Quick Start Guide

Get started with Deep Context in 5 minutes.

## Installation

```bash
cd examples/deep_context
cargo build --release

# Optional: Add to PATH
export PATH="$PATH:$(pwd)/target/release"
```

## 1. Initialize

Navigate to your Git repository and initialize Deep Context:

```bash
cd your-project
deep-context init
```

This creates:
- `.deep-context/` directory (add to `.gitignore`)
- Git hooks for automatic decision tracking
- Configuration file

## 2. Capture Your First Decision

Use the interactive mode for easiest input:

```bash
deep-context capture --interactive
```

Or provide all details via CLI:

```bash
deep-context capture \
  --title "Your Decision Title" \
  --context "Why did you need to make this decision?" \
  --decision "What did you decide?" \
  --rationale "Why is this the best choice?" \
  --alternative "Other option you considered" \
  --consequence "What are the tradeoffs?" \
  --files "src/affected/files/**" \
  --tag "architecture"
```

## 3. Query Decisions

Search with natural language:

```bash
deep-context query "authentication"
```

Filter by tags:

```bash
deep-context query --tag architecture
```

Find decisions for specific files:

```bash
deep-context query --file "src/auth/handler.rs"
```

## 4. View Timeline

See how decisions evolved:

```bash
deep-context timeline --visual
```

## 5. Export Documentation

Generate Markdown docs:

```bash
deep-context export --format markdown --output docs/decisions/
```

## Common Workflows

### Documenting a Major Refactor

```bash
# Before starting refactor
deep-context capture --interactive
# Answer prompts about what you're changing and why

# During refactor, reference the decision in commits
git commit -m "Refactor auth system

Relates-To: ADR-042"

# After completion, see the full timeline
deep-context timeline --decision ADR-042
```

### Onboarding New Developers

```bash
# Show all architectural decisions
deep-context query --tag architecture

# Export as documentation
deep-context export --format markdown --output docs/adr/

# New developer can read docs/adr/README.md for overview
```

### Pre-commit Checklist

```bash
# See which decisions relate to your changes
FILES=$(git diff --cached --name-only)
deep-context suggest-decisions $FILES

# If major change, consider documenting
deep-context capture --interactive
```

### Security Audit

```bash
# Find all security-related decisions
deep-context query --tag security

# Export for auditors
deep-context export --format markdown --output security-decisions/
```

## Tips

### Good Decision Titles
- ✅ "Use Redis for Session Storage"
- ✅ "Migrate from REST to GraphQL"
- ✅ "Implement End-to-End Encryption"
- ❌ "Backend changes"
- ❌ "Update API"

### Writing Context
Explain the **situation** that made this decision necessary:
- What problem are you solving?
- What constraints do you have?
- What is the current state?

Example:
```
Our monolithic app deployment takes 30 minutes and blocks the entire team.
We have 15 developers making changes daily, leading to frequent conflicts.
Need to enable independent team deployments while maintaining system coherence.
```

### Writing Rationale
Explain **why** this decision is the best choice:
- What benefits does it provide?
- How does it solve the problem?
- Why is it better than alternatives?

Example:
```
Microservices allow teams to deploy independently, reducing coordination overhead.
Each service can scale based on its specific load patterns.
Teams can choose appropriate technologies for their domain.
Fault isolation prevents cascading failures.
```

### Documenting Alternatives
List options you seriously considered and **why you rejected them**:

```bash
--alternative "Keep monolith but improve CI/CD: Faster builds but doesn't solve team coordination"
--alternative "Use serverless functions: Would work but team lacks AWS Lambda experience"
--alternative "Complete rewrite: Too risky, would take 6 months with no new features"
```

### Linking Files

Be specific about affected code:
```bash
--files "src/auth/**"           # All auth code
--files "src/api/graphql/**"    # GraphQL implementation
--files "Dockerfile"             # Configuration
--files "docs/api.md"            # Documentation
```

### Using Tags

Create a tagging convention for your team:

**By Domain:**
- `architecture`, `api`, `database`, `infrastructure`

**By Type:**
- `security`, `performance`, `scalability`, `compliance`

**By Technology:**
- `react`, `rust`, `postgresql`, `kafka`, `redis`

**By Status:**
- `experiment`, `production`, `deprecated`

## Git Integration

### Automatic Linking

When you commit, reference decisions in your commit message:

```bash
git commit -m "Implement JWT authentication

Relates-To: ADR-007
- Add JWT token generation
- Implement refresh token flow
- Add token validation middleware"
```

The Git hook automatically links the commit to ADR-007.

### Pre-commit Suggestions

The `prepare-commit-msg` hook suggests relevant decisions based on changed files:

```bash
git add src/auth/*
git commit
# Editor opens with:
#
# Add your commit message here
#
# Suggested decision references:
# ADR-007 - JWT Authentication Implementation
# ADR-012 - API Security Best Practices
# Use 'Relates-To: ADR-XXX' to link this commit
```

## Troubleshooting

### "Deep Context not initialized"

```bash
# Make sure you're in a Git repository
git status

# Initialize Deep Context
deep-context init
```

### "Permission denied" on hooks

```bash
# Make hooks executable
chmod +x .git/hooks/post-commit
chmod +x .git/hooks/prepare-commit-msg
```

### Database lock error

Only one instance can access the database at a time. Close other `deep-context` processes.

### Can't find decisions

```bash
# Check statistics
deep-context stats

# List all decisions
deep-context query

# Check if decisions were saved
ls .deep-context/db/
```

## Next Steps

- Read [EXAMPLE.md](EXAMPLE.md) for a detailed real-world scenario
- Read [README.md](README.md) for complete documentation
- Set up CI/CD integration
- Create team tagging conventions
- Export existing architectural decisions from docs/wikis

## Getting Help

```bash
# General help
deep-context --help

# Help for specific command
deep-context capture --help
deep-context query --help
```

## Integration with AIngle

Deep Context is built on AIngle's technology stack:

- **aingle_graph**: Semantic triple store for decision relationships
- **aingle_ai**: (Future) AI-powered decision search and recommendations
- **Git integration**: Native Git repository support

In a full AIngle deployment, Deep Context can:
- Create RDF knowledge graphs of decisions
- Use semantic similarity to find related decisions
- Integrate with IoT/edge deployments
- Provide SPARQL query interface

## Best Practices

1. **Capture decisions early**: Don't wait until after implementation
2. **Be honest about tradeoffs**: Document the consequences
3. **Update status**: Mark decisions as superseded when architecture evolves
4. **Link commits**: Always reference decision IDs in commit messages
5. **Tag consistently**: Use a team-wide tagging convention
6. **Export regularly**: Generate documentation for wikis/portals
7. **Review timeline**: Periodically review decision evolution

## Example Daily Workflow

```bash
# Morning: See what decisions were made recently
deep-context query --since "7 days ago"

# Before big change: Document the decision
deep-context capture --interactive

# During work: Check related decisions
deep-context query --file "path/to/file"

# When committing: Reference the decision
git commit -m "Your change

Relates-To: ADR-XXX"

# End of day: Check statistics
deep-context stats
```

---

**You're now ready to preserve the "why" behind your code!**

Start capturing decisions today, and your future self (and teammates) will thank you.
