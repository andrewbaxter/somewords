# Somewords

A very tiny static page generator, from a git repo with markdown documents at the top level.

This only deals with data in the top level of the repo. All markdown documents are turned into pages, with an index page pointing to the first. All non-markdown documents are copied as-is.

You need a `logo.svg` in the same directory as your documents.

## Use it

### Binary

1. `cargo install somewords`
2. `somewords "My Blog" https://github.com/my/repo/commits/`

The pages will be generated in `pages/`.

### Github Actions

Paste this action

```yaml
blah
```

## Customization

- Create a `footer.md` which will get appended to all pages in a separate footer section
- Create a `index.css` dir in the top level with an `index.css` - this will be used instead of the built in css
