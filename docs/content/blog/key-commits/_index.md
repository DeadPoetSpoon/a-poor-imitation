+++
title = "关键提交"
sort_by = "weight"
template = "series.html"
transparent = true

[extra]
series = true

[extra.series_intro_templates]
default = "本文是 $SERIES_HTML_LINK 系列的一部分。"

[extra.series_outro_templates]
next_only = "欢迎！下一篇：$NEXT_HTML_LINK"
middle = "上一篇：$PREV_HTML_LINK | 下一篇：$NEXT_HTML_LINK"
prev_only = "终章！上一篇：$PREV_HTML_LINK"
default = "第 $SERIES_PAGE_INDEX 部分，共 $SERIES_PAGES_NUMBER 部分"
+++
