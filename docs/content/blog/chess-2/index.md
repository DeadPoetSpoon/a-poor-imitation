+++
title = "国际象棋 TWO"
date = 2024-08-07
updated = 2025-07-28
description = "继续优化！"

[taxonomies]
tags = ["学习", "实践"]
+++

# 重新出发！

目前或许是重新开始的绝佳机会。

# 更新 `Bevy`

先更新 Bevy 到 v0.16.0 吧！

主要有两处修改

1. 修改资源加载方式。
1. 修改`Camera2D`实体添加方式。

# 修改BUG

当非自己回合时，左键选中右键移动，若目标位置有棋子时，会触发吃子的效果。

只需修改 `system.rs` 中 `move_pieces_system` 移动系统的一小部分代码即可。

```rust
if let Some(select_color) = select_color {
    for (_pieces, position, mut color) in &mut query {
        if position.row == move_position.row && position.col == move_position.col {
            if color.kind == select_color {
                game_state.move_position = None;
                return;
            } else {
                // 在移除棋子前判断当前回合即可。
                if color.kind != game_state.current_turn {
                    color.kind = ChessColorKind::Gray;
                }
            }
        }
    }
}
```
