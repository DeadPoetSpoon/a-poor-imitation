+++
title = "GitHub"
date = 2023-11-08
updated = 2023-11-08
description = "整个项目将使用 GitHub 作为仓库管理工具"

[taxonomies]
tags = ["学习", "GitHub", "工具"]
+++

## GitHub Tokens

现在强制使用 [GitHub Tokens](https://github.com/settings/tokens) 管理仓库，毫无疑问安全了很多。

## GitHub Pages

API Docs 托管在 [GitHub Pasges](https://DeadPoetSpoon.github.io/a-poor-imitation/) 。

## GitHub Actions

API Docs 使用 [Zola](https://www.getzola.org/) 作为静态网站生成器，并利用 [GitHub Actions](https://github.com/DeadPoetSpoon/a-poor-imitation/actions) 在每次 `Push` 时自动构建与部署。

按照 [官方文档](https://www.getzola.org/documentation/deployment/github-pages/#github-actions) 进行了相应配置，但是发现 [shalzz/zola-deploy-action](https://github.com/shalzz/zola-deploy-action) 使用官方仓库发布的 Zola，并不支持在构建时建立中文索引（需要额外的特性：indexing-zh），于是在其基础上创建 [DeadPoetSpoon/zola-deploy-action](https://github.com/DeadPoetSpoon/zola-deploy-action) 仓库提供该功能。
