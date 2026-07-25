# Trading Engine Demo

历史数据回测框架，计算移动平均线(MA)和 RSI 动量指标，执行双均线交叉策略。

## 功能

- 简单移动平均线 (SMA) 计算，支持任意周期
- RSI (Relative Strength Index) 动量指标，识别超买超卖
- 双均线交叉策略（金叉买入 / 死叉卖出）
- 交易统计：总收益、胜率、最大单笔盈亏

## 用法

```bash
dalib run demos/trading_engine/main.dal
```

## 输出示例

```
=== Trading Engine Backtest Report ===
Strategy: MA Cross (short=3, long=7)
Price data points: 20
Price range: 100.0 → 117.0

Calculating Moving Averages...
  SMA(3) length: 18
  SMA(7) length: 14
Calculating RSI...
  RSI(5) length: 15
  Latest RSI: 65.3

Running Backtest...
  Trades:  BUY at 104.0
  SELL at 105.5 P&L=1.5
  ...

=== Backtest Results ===
Total trades: 3
Wins: 2
Losses: 1
Win rate: 66.67%
Total P&L: 5.0
```