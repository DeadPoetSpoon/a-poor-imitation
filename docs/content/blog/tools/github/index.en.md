+++
title = "GitHub"
date = 2023-11-08
updated = 2023-11-08
description = "The entire project will use GitHub as its repository management tool"
taxonomies = { tags = ["learning", "tools"] }
+++

> **🤖 AI Translation Notice**: This post has been translated from Chinese to English by an AI.

## GitHub Tokens

Now it's mandatory to use [GitHub Tokens](https://github.com/settings/tokens) to manage repositories — undoubtedly much more secure.

## GitHub Pages

API Docs is hosted on [GitHub Pages](https://DeadPoetSpoon.github.io/a-poor-imitation/).

## GitHub Actions

API Docs uses [Zola](https://www.getzola.org/) as a static site generator, and leverages [GitHub Actions](https://github.com/DeadPoetSpoon/a-poor-imitation/actions) to automatically build and deploy on every `Push`.

I configured it according to the [official documentation](https://www.getzola.org/documentation/deployment/github-pages/#github-actions), but discovered that [shalzz/zola-deploy-action](https://github.com/shalzz/zola-deploy-action) uses the Zola binary from the official repository and doesn't support building Chinese search indexes (which requires the extra feature: indexing-zh). So I created [DeadPoetSpoon/zola-deploy-action](https://github.com/DeadPoetSpoon/zola-deploy-action) based on it to provide that functionality.
