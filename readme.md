# Somewords

A very tiny static page generator, from a git repo with markdown documents at the top level.

You need a `logo.svg` in the same directory as your documents.

## Use it

### Binary

1. `cargo install unblog`
2. `unblog "My Blog" https://github.com/my/repo/commits/`

The pages will be generated in `pages/`.

### Github Actions

Paste this action

```yaml
blah
```

## Customization

- Create your own `style.css` file and place it alongside your documents
