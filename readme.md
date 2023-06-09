# Somewords

A very tiny static page generator, from a git repo with markdown documents.

## How it works

When you call somewords, it assumes the current directory is the root of the repository. It only looks in the root for documents, so you can put WIP documents in a subdirectory if you'd like.

You need a `logo.svg` in the same directory as your documents.

It does the following steps:

- Creates a `pages/` output directory
- Copies all non-`.md` files to the output directory
- If it didn't copy an `index.css`, it generates the default style files including `index.css`
- It translates all markdown files into html, and generates an index that redirects to the latest

## Use it

### Binary

From the top level of your repository which is where your markdown documents must be located,

1. `cargo install somewords`
2. `somewords "My Blog" https://github.com/my/repo/commits/`

The pages will be generated in `pages/`.

### Github Actions

Paste this in `.github/workflows/pages.yaml`, updating the args as necessary:

```yaml
name: Pages
on:
  push:
    branches:
      - "master"
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: docker://ghcr.io/andrewbaxter/somewords:latest
        with:
          args: /somewords 'Andrew Baxter' https://github.com/andrewbaxter/5987/commit/
      - uses: actions/upload-artifact@v3
        with:
          name: github-pages
          path: pages
      - uses: actions/deploy-pages@v2
```

## Customization

- Use `--color-bg 200` or `--color-accent 60` to colorize the default style (see `--help`)
- Create a `footer.md` which will get appended to all pages in a separate footer section
- Create a `index.css` dir in the top level with an `index.css` - this will be used instead of the built in css
