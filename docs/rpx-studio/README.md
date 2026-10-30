# 报表工作台 · RPX Studio

浏览器里四步：**传数据 → 选骨架 → 说需求 → 出结果**。
中间那步「说需求」由本地编译器把自然语言翻成 rpx 模板，再由本机的 rpx 引擎真跑出表格、CSV、xlsx 和打印版。

不上传外网、不调模型接口、不需要登录。数据只落在 `work/<会话>/`。

## 跑起来

```bash
cd F:/project/admin/docs/rpx-studio
python server.py            # 默认 127.0.0.1:8777
```

然后打开 <http://127.0.0.1:8777>。

引擎路径默认是 `F:\project\report\rpx\target\debug\rpx.exe`，换机器时用环境变量指定：

```bash
RPX_BIN=D:\somewhere\rpx.exe python server.py
```

只用标准库，Python 3.11+ 都能跑，无需 pip install。

## 四步都发生了什么

| 步骤 | 界面做的事 | 背后 |
|---|---|---|
| 1 上传 | 拖入 CSV，显示行列摘要与前 8 行 | 自动判编码（UTF-8 / GBK）与分隔符（, ; Tab \|），推断每列类型，猜分组/明细/数值三个角色 |
| 2 选骨架 | 三张卡：分组排名表 / 分组汇总表 / 明细清单 | 每个骨架是一组固定特征，决定版面和默认要算的列 |
| 3 说需求 | 一段话 + 几个开关 | 关键词映射成功能（排名 / 组内占比 / 全表占比 / 第一标记 / 合计）；需求里点到的列名会改写角色分配；右侧同步给出编译决策日志与最终列 |
| 4 出结果 | 状态条 + 报表 + 下载 | 真调 `rpx lint` / `rpx template validate` / `rpx run --export csv --export xlsx --export print-xlsx` |

状态条上的每一项都来自引擎返回：`lint` 的告警条数、`validate` 的 checked/deferred、`run` 的退出码与分页。

## 「说需求」能听懂什么

| 说法 | 效果 |
|---|---|
| 排名 / 名次 / 排位 | 加「组内排名」列（竞赛排名，并列后跳号） |
| 组内占比 / 占比 / 比重 / 比例 | 加「组内占比」列，分母是同组合计 |
| 全表 / 整体 + 占比 | 加「占比」列，分母是全表合计 |
| 第一 / 冠军 / 标出来 | 加「组内第一」标记（会自动补上排名列） |
| 合计 / 总计 / 汇总 | 加合计行 |
| 按「某个列名」分组 | 把该列改成分组维 |
| 点数值列名 + 求和/排名/占比 | 把该列改成数值维 |

## 目录

```
rpx-studio/
  server.py        本地服务（标准库 HTTP，~380 行）
  compiler.py      CSV → 模板 的编译器（纯函数，可单测）
  index.html       单页界面（无构建、无外部依赖）
  test_compile.py  对编译器做真跑验证：5 组用例 × lint/validate/run/独立重算
  work/            运行时会话（自动只留最近 30 个）
```

## 已验证

```
python test_compile.py                                  # 用示例 CSV
python test_compile.py F:/tmp/studio-data/payroll.csv    # 换成中文列名
```

两组数据各 5 个编译用例（满配 / 只排名 / 无合计 / 汇总 / 清单），每个都过四道：
`lint` 无告警、`validate` valid、`run` exit 0、**按源 CSV 独立重算后逐格比对一致**。

浏览器端两套 E2E 共 37 项断言：角色推断、卡片选中、需求识别、决策日志、报表渲染（含大区纵向合并）、合计与百分比格式、产物下载、示例数据按钮、无前端报错。

## 边界（写清楚，不装）

- **这是规则编译器，不是模型**。同样的输入永远得到同样的模板，可复现、可审计；但它只认上表那些说法，不处理开放式需求。要接模型，替换 `server.py` 里的 `detect_intent` / `detect_columns` 即可，`/api/compile` 的出入参不用动。
- 只支持**单分组 + 单明细 + 单度量**。多级分组、交叉表、同比环比都还没有。
- 三个骨架覆盖的是「分组比较」这类表。资产负债表那种左右勾稽、多段小计的版面不在范围内。
- 不写回数据库。产物落在 `work/<会话>/run/artifacts/`，下载走 `/api/download`。

## 接口

| 方法 | 路径 | 说明 |
|---|---|---|
| POST | `/api/upload` | 请求体是 CSV 原始字节，文件名放 `X-Filename`（URL 编码） |
| POST | `/api/compile` | `{token, recipe, roles, features, need, title}` → 模板 + 决策日志 + 列 |
| POST | `/api/run` | `{token, template}` → lint/validate/run 结果 + 结果格 + 产物清单 |
| GET | `/api/download?token=&f=` | 取产物 |
| GET | `/api/sample` | 示例数据（10 家门店） |
| GET | `/api/health` | 引擎是否在位 |
