# Schalentier Package Registry

This directory contains the community-maintained package registry for schalentier.

## Structure

- `packages.json` - Main registry file containing all package definitions

## Contributing

To add a new package:

1. Edit `packages.json`
2. Add your package to the `packages` array
3. Run `schalentier registry validate` to check syntax
4. Submit a PR

## Package Entry Format

```json
{
  "name": "package-name",
  "aliases": ["alternative-name"],
  "description": "Short description (max 200 chars)",
  "keywords": ["relevant", "keywords"],
  "providers": {
    "binary": {
      "name": "binary-name",
      "repo": "github-user/repo-name"
    },
    "conda": {
      "name": "conda-package-name"
    },
    "cargo": {
      "name": "crate-name"
    },
    "pnpm": {
      "name": "npm-package-name"
    },
    "paru": {
      "name": "arch-package-name"
    },
    "brew": {
      "name": "homebrew-formula-name"
    }
  }
}
```

## Guidelines

- Use lowercase for `name` and `aliases`
- Include at least 2 providers if available
- Use canonical names (check existing packages for patterns)
- For npm scoped packages, use format: `@scope/package-name`
- For providers that don't have the package, you can set `"available": false` with a `reason`
