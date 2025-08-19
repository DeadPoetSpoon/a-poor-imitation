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

# UI

按照计划，先添加一些UI，`bevy` 的UI结构为一个 `Node` 加上UI要素。

那就先添加 轮次、步骤、控制 三个UI元素吧。

## 轮次

只需要一行字表示，UI实体由 一个用来区分的 `Name` 组件、一个 `Node` 组件、一个 `Text` 组件 组成。

```rust
commands.spawn((
    Name::new("ui_turn"),
    Text::new("Current Turn: White"), // default White first
    Node {
        top: Val::Px(100.0),
        left: Val::Px(200.0),
        ..Default::default()
    },
));
```

## 步骤

与轮次一样，不过加上 `BackgroundColor` 组件渲染了背景色，还添加一些滚动控制，暂时用不了。

每一步将作为其子节点，这样步骤将全部渲染至黑色背景中，并且按照设定顺序排列。

和前端一样的逻辑。

```rust
commands.spawn((
    Name::new("ui_steps"),
    Node {
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Start,
        align_items: AlignItems::Start,
        align_self: AlignSelf::Stretch,
        overflow: Overflow::scroll(),
        width: Val::Px(400.0),
        height: Val::Px(550.0),
        left: Val::Px(200.0),
        top: Val::Px(200.0),
        ..Default::default()
    },
    BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
    children![(
        Node {
            min_height: Val::Px(22.0),
            max_height: Val::Px(22.0),
            ..default()
        },
        children![(
            Text(format!("Not Start")),
            Label,
            AccessibilityNode(Accessible::new(Role::ListItem)),
        ),],
    ),],
));
```

## 控制

添加一个关闭的按钮。给按钮添加了边框、圆弧、和文字，同样是子节点的逻辑，使文字在按钮中。

```rust
commands.spawn((
    Name::new("close_button"),
    Button,
    Node {
        width: Val::Px(150.0),
        height: Val::Px(65.0),
        left: Val::Px(200.0),
        top: Val::Px(800.0),
        border: UiRect::all(Val::Px(5.0)),
        // horizontally center child text
        justify_content: JustifyContent::Center,
        // vertically center child text
        align_items: AlignItems::Center,
        ..default()
    },
    BorderColor(Color::WHITE),
    BorderRadius::all(Val::Px(10.0)),
    BackgroundColor(Color::BLACK),
    children![(
        Text::new("Close"),
        TextColor(Color::srgb(0.9, 0.9, 0.9)),
        TextShadow::default(),
    )],
));
```

## 效果

简单UI的效果就实现啦。

![ui_startup](./ui_startup.png)
