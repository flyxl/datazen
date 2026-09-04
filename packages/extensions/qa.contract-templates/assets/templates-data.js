/**
 * 测试环境合同模板中心 - 预设环境与模板数据集
 */
(function () {
  'use strict';

  var ENVIRONMENTS = {
    'qa-01': {
      id: 'qa-01',
      name: 'QA-01 综合测试环境',
      code: 'QA01',
      color: '#3B82F6',
      badge: 'QA-综合测试',
      partyA: {
        name: '杭州知数云测信息技术有限公司（测试主体）',
        uscc: '91330100TEST000188',
        legalPerson: '张测试',
        address: '浙江省杭州市余杭区文一西路测试中心 8 号楼 301 室',
        phone: '0571-88880001',
        bank: '招商银行杭州未来科技城支行',
        account: '5719 0000 8888 1001',
      },
      seal: {
        text: '杭州知数云测信息技术有限公司',
        code: 'QA-SEAL-2026-8801',
        type: '合同专用章 (测试专章)',
      },
      callbackUrl: 'https://mock-gateway.test.datazen.internal/contracts/callback',
    },
    'dev-01': {
      id: 'dev-01',
      name: 'DEV-01 开发联调环境',
      code: 'DEV01',
      color: '#10B981',
      badge: 'DEV-本地联调',
      partyA: {
        name: '知数先锋技术(开发测试专用)有限公司',
        uscc: '91330100DEV0000001',
        legalPerson: '李联调',
        address: '浙江省杭州市西湖区西溪数码港开发大厦 A 座 12F',
        phone: '0571-88880002',
        bank: '中国工商银行西湖支行',
        account: '1202 0000 0000 8802',
      },
      seal: {
        text: '知数先锋技术开发专用章',
        code: 'DEV-SEAL-2026-0001',
        type: '开发测试电子签',
      },
      callbackUrl: 'http://localhost:8080/api/v1/contract-events',
    },
    staging: {
      id: 'staging',
      name: 'STAGING 预发布灰度环境',
      code: 'STG',
      color: '#F59E0B',
      badge: 'STG-准生产演练',
      partyA: {
        name: '浙江数研云联智能科技有限公司（灰度演练）',
        uscc: '91330100STAGE999901',
        legalPerson: '王准发',
        address: '浙江省杭州市滨江区江汉路 1788 号数研智创中心',
        phone: '0571-88880003',
        bank: '中国建设银行杭州江南支行',
        account: '3305 0000 9999 0003',
      },
      seal: {
        text: '浙江数研云联灰度演练专用章',
        code: 'STG-SEAL-2026-9901',
        type: '预发电子业务章',
      },
      callbackUrl: 'https://stage-contracts.datazen.internal/callback',
    },
    uat: {
      id: 'uat',
      name: 'UAT 业务验收测试环境',
      code: 'UAT',
      color: '#8B5CF6',
      badge: 'UAT-用户验收',
      partyA: {
        name: '数舟云端互联商务有限公司（UAT专户）',
        uscc: '91330100UAT7777771',
        legalPerson: '赵验收',
        address: '上海市浦东新区张江高科园区祥科路 298 号',
        phone: '021-68880004',
        bank: '交通银行上海张江支行',
        account: '3100 0000 7777 0004',
      },
      seal: {
        text: '数舟云端互联商务有限公司',
        code: 'UAT-SEAL-2026-7701',
        type: '业务验收测试章',
      },
      callbackUrl: 'https://uat-bridge.datazen.internal/webhook/contract',
    },
  };

  var TEMPLATES = [
    {
      id: 'tpl-saas-procurement',
      code: 'TPL-SAAS-01',
      title: '企业级 SaaS 软件订阅及技术支持服务合同',
      category: 'software',
      categoryName: '软件订阅与服务',
      version: 'v2.4',
      status: '已定稿',
      description:
        '涵盖云端数据协同平台订阅席位、SLA 服务等级协议、私有节点部署支持与测试联调维护条款。',
      updatedAt: '2026-08-20',
      variables: [
        { key: 'contract_no', label: '合同编号', default: 'DZ-SAAS-2026-QA091', type: 'text' },
        {
          key: 'party_a',
          label: '甲方 (买方)',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        {
          key: 'party_b',
          label: '乙方 (服务商)',
          default: '深圳海纳数智互联科技股份有限公司',
          type: 'text',
        },
        { key: 'effective_date', label: '生效起始日', default: '2026-09-01', type: 'date' },
        { key: 'expire_date', label: '服务截止日', default: '2027-08-31', type: 'date' },
        {
          key: 'saas_edition',
          label: '订购版本',
          default: 'DataZen Enterprise 旗舰版 (云协同)',
          type: 'select',
          options: ['标准企业版', 'DataZen Enterprise 旗舰版 (云协同)', '金融专区私有化部署版'],
        },
        { key: 'seat_count', label: '席位配额 (个)', default: '100', type: 'number' },
        {
          key: 'sla_level',
          label: 'SLA 级别',
          default: '99.95% (响应时长 ≤ 15分钟)',
          type: 'text',
        },
        { key: 'amount', label: '合同总金额 (元)', default: '258000.00', type: 'amount' },
        { key: 'amount_words', label: '总金额大写', default: '贰拾伍万捌仟元整', type: 'text' },
        { key: 'contact_a', label: '甲方负责人', default: '陈主管', type: 'text' },
        { key: 'phone_a', label: '甲方电话', default: '13800000001', type: 'text' },
        { key: 'contact_b', label: '乙方负责人', default: '周经理', type: 'text' },
        { key: 'phone_b', label: '乙方电话', default: '13912345678', type: 'text' },
      ],
      content: [
        '# 企业级 SaaS 软件订阅及技术支持服务合同',
        '',
        '**合同编号**：{{contract_no}}  ',
        '**签署地点**：中国 · 杭州市余杭区  ',
        '**生效日期**：{{effective_date}}  ',
        '',
        '---',
        '',
        '### 签约双方',
        '',
        '- **甲方（采购方）**：{{party_a}}',
        '  - 统一社会信用代码/测试主体号：{{party_a_uscc}}',
        '  - 法定代表人/授权代表：{{contact_a}}（联系电话：{{phone_a}}）',
        '',
        '- **乙方（服务提供商）**：{{party_b}}',
        '  - 统一社会信用代码：91440300MA5FTEST99',
        '  - 项目业务负责人：{{contact_b}}（联系电话：{{phone_b}}）',
        '',
        '鉴于甲方因研发测试与数据运营业务需要，拟向乙方采购 SaaS 软件在线订阅服务与专业技术保障；乙方具备提供相关专业软件产品及技术支持的合法资质与服务能力。双方在平等互利、诚实信用的原则下，经友好协商订立本协议。',
        '',
        '### 第一条 订购服务内容与规格',
        '',
        '1. **订购产品版本**：乙方应向甲方提供 **{{saas_edition}}** 的全功能开通授权及配套云端算力调度权限。',
        '2. **账号配额**：本期采购正式开通并发用户席位共计 **{{seat_count}}** 个。甲方可根据研发团队规模在管理后台自主分配或注销角色权限。',
        '3. **测试联调沙箱环境**：服务期内，乙方须向甲方免费开放独立沙箱环境一套，用于系统集成自动化测试，沙箱环境数据与生产环境严格物理隔离。',
        '',
        '### 第二条 服务周期与 SLA 承诺',
        '',
        '1. **服务期限**：自 **{{effective_date}}** 起至 **{{expire_date}}** 止。',
        '2. **服务可用度指标**：乙方承诺在约定期限内，线上服务月度可用性达 **{{sla_level}}**。',
        '3. **故障分级响应机制**：',
        '   - P0/P1 级重大系统阻断（影响全部或核心查询）：15 分钟内响应，2 小时内提供临时恢复方案；',
        '   - P2/P3 级常规缺陷及咨询：工作时间内 1 小时内响应，24 小时内完成排查并给出发布计划。',
        '',
        '### 第三条 费用及结算方式',
        '',
        '1. **服务总费用**：人民币 **￥{{amount}} 元**（大写：**{{amount_words}}**）。该费用已包含软件订阅许可费、测试沙箱搭建及维保技术支持税费。',
        '2. **付款进度**：',
        '   - **第一期首付款（60%）**：合同签订且乙方交付初始管理员凭证及测试联调接入文档后 10 个工作日内支付；',
        '   - **第二期尾款（40%）**：系统平稳运行满 90 自然日，并完成第一阶段阶段性联调验收后 10 个工作日内结清。',
        '',
        '### 第四条 数据安全与保密承诺',
        '',
        '1. 甲方在平台留存的业务数据、查询历史、测试结构模型均归甲方独家所有，乙方不得非法转存、镜像或向任何第三方披露。',
        '2. 数据传输全程采用 TLS 1.3 强加密标准，敏感测试鉴权秘钥存储满足国密 SM4 或 AES-256 标准。',
        '',
        '### 第五条 争议解决与签署',
        '',
        '本合同一式两份，甲乙双方各执一份，经双方加盖测试电子印章或合同专用章后生效。因本协议履行发生的争议，双方应友好协商，协商不成的，由甲方所在地有管辖权的人民法院诉讼解决。',
      ].join('\n'),
    },
    {
      id: 'tpl-dev-commission',
      code: 'TPL-DEV-02',
      title: '定制化软件系统开发与项目实施委托协议',
      category: 'development',
      categoryName: '委托开发与交付',
      version: 'v3.1',
      status: '已定稿',
      description:
        '适用于按里程碑交付的定制化数据引擎开发项目，含联调验收测试周期与知识产权归属条款。',
      updatedAt: '2026-08-25',
      variables: [
        { key: 'contract_no', label: '合同编号', default: 'DZ-DEV-2026-0922', type: 'text' },
        {
          key: 'party_a',
          label: '甲方 (委托方)',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        {
          key: 'party_b',
          label: '乙方 (受托方)',
          default: '北京极客引擎软件工程技术研究院有限公司',
          type: 'text',
        },
        {
          key: 'project_name',
          label: '委托开发项目名称',
          default: '分布式混合异构数据库同步引擎组件 (V3)',
          type: 'text',
        },
        { key: 'milestone_count', label: '交付里程碑阶段数', default: '3', type: 'number' },
        { key: 'acceptance_days', label: '测试验收考核期 (天)', default: '15', type: 'number' },
        { key: 'warranty_months', label: '免费质保维保期 (月)', default: '12', type: 'number' },
        { key: 'amount', label: '委托开发酬金 (元)', default: '480000.00', type: 'amount' },
        { key: 'amount_words', label: '总额大写', default: '肆拾捌万元整', type: 'text' },
        { key: 'effective_date', label: '签订日期', default: '2026-09-04', type: 'date' },
      ],
      content: [
        '# 定制化软件系统开发与项目实施委托协议',
        '',
        '**合同编号**：{{contract_no}}  ',
        '**项目名称**：{{project_name}}  ',
        '**签订日期**：{{effective_date}}  ',
        '',
        '---',
        '',
        '### 协议当事人',
        '- **委托方（甲方）**：{{party_a}}',
        '- **受托开发方（乙方）**：{{party_b}}',
        '',
        '### 第一条 委托开发任务与交付规格',
        '',
        '1. 甲方委托乙方进行 **{{project_name}}** 的方案设计、核心驱动适配模块编码开发及单元与集成测试。',
        '2. 交付物清单包括：系统源码工程（含完整注释）、Rust/C++ 动态链接库、API 设计白皮书及完整的黑盒/白盒自动化测试用例套件。',
        '',
        '### 第二条 项目里程碑与联调排期',
        '',
        '项目共分 **{{milestone_count}}** 个里程碑进行分段验收与付款：',
        '- **阶段一（架构设计与协议层）**：完成核心协议抽象及 DriverRegistry 动态工厂，完成开发环境自测。',
        '- **阶段二（同族引擎与并发管道）**：完成 Data Sync 比较与执行器开发，并在测试环境中通过跨节点 1000 万级数据量压力测试。',
        '- **阶段三（宿主联调与终验交付）**：完成桌面前端与桌面子窗口双端交互验证，交付全量产物。',
        '',
        '### 第三条 测试联调与验收标准',
        '',
        '1. 乙方提交交付物后，甲方在测试环境开展为期 **{{acceptance_days}}** 个工作日的严格联调验收测试。',
        '2. 验收通过标准：所有自动化 E2E 契约用例测试全数通过（Pass 100%），且核心组件连续 72 小时无内存泄漏与进程崩溃（Crash）。',
        '',
        '### 第四条 酬金与结算',
        '',
        '开发总酬金为人民币 **￥{{amount}} 元**（大写：**{{amount_words}}**）。',
        '- 签署合同后支付预付款 30%；',
        '- 里程碑二通过测试环境评审后支付进度款 40%；',
        '- 终验合格且交付完备源码后支付 30%。',
        '',
        '### 第五条 知识产权与售后保证',
        '',
        '1. 乙方为本项目定制编写的全部业务代码及技术产物知识产权归甲方完全独占所有。',
        '2. 验收合格之日起，乙方提供为期 **{{warranty_months}}** 个月的免费缺陷修复与版本兼容质保保障。',
      ].join('\n'),
    },
    {
      id: 'tpl-nda-confidential',
      code: 'TPL-NDA-03',
      title: '商业秘密保护与双向保密协议 (NDA)',
      category: 'confidential',
      categoryName: '保密协议与法务',
      version: 'v1.8',
      status: '已定稿',
      description: '用于技术合作对接、敏感测试数据样本传输、架构源码交流前的双向保密约束协议。',
      updatedAt: '2026-08-10',
      variables: [
        { key: 'contract_no', label: '协议编号', default: 'DZ-NDA-2026-0081', type: 'text' },
        {
          key: 'disclosing_party',
          label: '披露方 (Party A)',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        {
          key: 'receiving_party',
          label: '接收方 (Party B)',
          default: '上海数聚前沿信息技术合伙企业(有限合伙)',
          type: 'text',
        },
        {
          key: 'purpose',
          label: '合作商谈与技术评测目的',
          default: '针对新一代数据库管理桌面端插件体系及测试沙箱联调对接',
          type: 'text',
        },
        { key: 'confidentiality_years', label: '保密存续期 (年)', default: '3', type: 'number' },
        {
          key: 'liquidated_damages',
          label: '违约赔偿金 (元)',
          default: '500000.00',
          type: 'amount',
        },
        { key: 'liquidated_words', label: '违约金大写', default: '伍拾万元整', type: 'text' },
        { key: 'effective_date', label: '签署日期', default: '2026-09-04', type: 'date' },
      ],
      content: [
        '# 商业秘密保护与双向保密协议 (NDA)',
        '',
        '**协议编号**：{{contract_no}}  ',
        '**签署生效日**：{{effective_date}}  ',
        '',
        '---',
        '',
        '### 缔约方',
        '- **甲方**：{{disclosing_party}}',
        '- **乙方**：{{receiving_party}}',
        '',
        '### 鉴于',
        '双方正在就 **{{purpose}}** 展开深入技术交流与商务联调合作。在合作期间，任一方（披露方）可能向另一方（接收方）披露具有保密性或专有性质的技术、业务与测试架构信息。为此双方特订立本保密协议。',
        '',
        '### 第一条 保密信息的范畴',
        '1. “保密信息”指由一方以口头、书面、电子载体或软件工程形式披露的非公开信息，包括但不限于：数据库连接凭据规范、未公开的 IPC 通信信令、架构拓扑白皮书、测试环境拓扑配置、算法源码、未公开财务与客户清单等。',
        '2. 测试环境中的脱敏测试数据集、模拟合同模板及仿真 Mock 规则，若带有商业规则标记，亦均视为保密资产。',
        '',
        '### 第二条 接收方的保密义务',
        '1. 接收方仅可将保密信息用于约定的 **{{purpose}}** 之唯一目的。',
        '2. 接收方应采取不低于保护自身同类重要商业秘密之合理审慎程度（且不低于行业通行标准）予以保护。',
        '3. 接收方仅限向确需知悉该信息的员工或专业法律/技术顾问披露，并确保上述人员签署不低于本协议严格程度的保密承诺。',
        '',
        '### 第三条 保密期限',
        '自信息披露之日起，保密义务持续有效 **{{confidentiality_years}}** 年；对于属于核心底层技术或国家级法律法规认定的商业秘密，保密义务应无限期有效直至该信息合法进入公共领域。',
        '',
        '### 第四条 违约救济与赔偿',
        '任何一方若违反本协议约定擅自泄露保密信息，应立即停止侵权，并向守约方支付违约金人民币 **￥{{liquidated_damages}} 元**（大写：**{{liquidated_words}}**）；违约金不足以弥补守约方因泄密遭受的直接与间接经济损失的，违约方应全额足额赔偿。',
      ].join('\n'),
    },
    {
      id: 'tpl-cloud-infra',
      code: 'TPL-INFRA-04',
      title: '云端弹性算力与专线网络基础设施租用协议',
      category: 'infrastructure',
      categoryName: '基础设施与算力',
      version: 'v2.0',
      status: '已定稿',
      description: '测试集群多区域高可用节点租赁协议，包含 SLA 扣减赔付梯度与网络带宽资源配额。',
      updatedAt: '2026-08-15',
      variables: [
        { key: 'contract_no', label: '合同编号', default: 'DZ-INFRA-2026-077', type: 'text' },
        {
          key: 'party_a',
          label: '甲方 (租用方)',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        {
          key: 'party_b',
          label: '乙方 (云服务商)',
          default: '天翼云极互联网络科技有限公司',
          type: 'text',
        },
        {
          key: 'region_cluster',
          label: '测试部署区域',
          default: '华东-杭州云测可用区 (Zone-A/Zone-B)',
          type: 'text',
        },
        { key: 'bandwidth_mbps', label: 'BGP 弹性专线带宽 (Mbps)', default: '500', type: 'number' },
        { key: 'availability_rate', label: '可用性承诺率', default: '99.99%', type: 'text' },
        { key: 'monthly_fee', label: '月度机房基础费 (元)', default: '16800.00', type: 'amount' },
        { key: 'total_amount', label: '年度预付总额 (元)', default: '201600.00', type: 'amount' },
        {
          key: 'amount_words',
          label: '年度总额大写',
          default: '贰拾万零壹仟陆佰元整',
          type: 'text',
        },
        { key: 'effective_date', label: '计费起始日期', default: '2026-09-01', type: 'date' },
      ],
      content: [
        '# 云端弹性算力与专线网络基础设施租用协议',
        '',
        '**合同编号**：{{contract_no}}  ',
        '**起始服务日**：{{effective_date}}  ',
        '',
        '### 签约主体',
        '- **甲方（承租方）**：{{party_a}}',
        '- **乙方（服务方）**：{{party_b}}',
        '',
        '### 一、资源配置与交付标准',
        '1. 乙方在 **{{region_cluster}}** 提供具备多活双链路容灾能力的物理宿主机及 Kubernetes 测试计算集群。',
        '2. 网络接入：提供 **{{bandwidth_mbps}} Mbps** 的双向独享 BGP 优质出口专线带宽，网络丢包率应 ≤ 0.05%。',
        '',
        '### 二、服务等级协议 (SLA) 与违约补偿',
        '1. 乙方承诺集群与网络月度综合可用性达到 **{{availability_rate}}**。',
        '2. 若因乙方原因单月可用性低于指标，乙方应在次月账单中按下表执行补偿折扣：',
        '   - 可用性在 99.0% 至 99.9% 之间：返还当月月费的 15%；',
        '   - 可用性在 95.0% 至 99.0% 之间：返还当月月费的 35%；',
        '   - 可用性低于 95.0%：返还当月全部服务费并承担排障紧急调度工时。',
        '',
        '### 三、计费与资金划扣',
        '本协议年化总金额为人民币 **￥{{total_amount}} 元**（大写：**{{amount_words}}**），按月计提基础费用 **￥{{monthly_fee}} 元**。按季度初首月前 5 个工作日内统一完成发票开具与对公转账支付。',
      ].join('\n'),
    },
    {
      id: 'tpl-labor-employment',
      code: 'TPL-HR-05',
      title: '高技术岗位劳动聘用与竞业限制合同',
      category: 'hr',
      categoryName: '人事雇佣与合规',
      version: 'v2.2',
      status: '标准版',
      description:
        '研发核心架构师/测试开发专家全日制聘用合同，包含严格试用期考察、期权授予与竞业协议。',
      updatedAt: '2026-08-30',
      variables: [
        { key: 'contract_no', label: '劳动合同编号', default: 'DZ-EMP-2026-018', type: 'text' },
        {
          key: 'employer',
          label: '用人单位 (甲方)',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        { key: 'employee_name', label: '受聘员工姓名 (乙方)', default: '刘子健', type: 'text' },
        { key: 'id_card', label: '身份证件号码', default: '33010619950312001X', type: 'text' },
        {
          key: 'job_title',
          label: '聘任岗位',
          default: '资深高可用数据库测试开发架构师 (P7)',
          type: 'text',
        },
        { key: 'department', label: '所属业务部门', default: '基础平台与质量工程部', type: 'text' },
        { key: 'base_salary', label: '月度基本薪酬 (元)', default: '36000.00', type: 'amount' },
        { key: 'probation_months', label: '试用期限 (月)', default: '3', type: 'number' },
        { key: 'nc_months', label: '离职竞业限制期 (月)', default: '12', type: 'number' },
        {
          key: 'nc_compensation',
          label: '月竞业补偿金比例',
          default: '原月薪的 40%',
          type: 'text',
        },
        { key: 'effective_date', label: '入职起聘日期', default: '2026-09-01', type: 'date' },
      ],
      content: [
        '# 高技术岗位劳动聘用与竞业限制合同',
        '',
        '**合同编号**：{{contract_no}}  ',
        '**签订日期**：{{effective_date}}  ',
        '',
        '### 签约双方',
        '- **用人单位（甲方）**：{{employer}}',
        '- **劳动者（乙方）**：{{employee_name}}（身份证号：{{id_card}}）',
        '',
        '### 第一条 岗位工作内容与地点',
        '1. 甲方聘用乙方在 **{{department}}** 担任 **{{job_title}}** 职务。',
        '2. 乙方的主要职责包括：自动化测试系统架构设计、跨平台驱动质量保障、研发联调测试环境治理及测试工具链建设。',
        '3. 劳动合同期限为 3 年，其中前 **{{probation_months}}** 个月为试用期。',
        '',
        '### 第二条 工作报酬与社保福利',
        '1. 乙方在正常出勤条件下的月度基本工资为人民币 **￥{{base_salary}} 元**。试用期薪资按正式标准的 100% 足额发放。',
        '2. 甲方按国家及浙江省杭州市规定为乙方依法缴纳五险一金，公积金缴纳比例执行上限 12%。',
        '',
        '### 第三条 知识产权归属与竞业限制',
        '1. **职务技术成果**：乙方在甲方任职期间履行职责开发或主要利用甲方物质技术条件完成的数据库工具、测试框架、专利发明等知识产权均归甲方独占享有。',
        '2. **离职竞业限制**：因乙方知悉核心架构，双方约定离职后 **{{nc_months}}** 个月内不得入职存在直接竞争关系的第三方机构。在此期间，甲方按月向乙方支付竞业补偿金（标准为 **{{nc_compensation}}**）。',
      ].join('\n'),
    },
    {
      id: 'tpl-license-extension',
      code: 'TPL-LIC-06',
      title: '商业数据库管理套件授权与持续维保续约协议',
      category: 'license',
      categoryName: '商业许可与维保',
      version: 'v1.5',
      status: '已定稿',
      description:
        '针对本地 IDE 插件与企业扩展商业授权的正式续约合同，明确升级权限与技术支持渠道。',
      updatedAt: '2026-08-18',
      variables: [
        { key: 'contract_no', label: '授权合同编号', default: 'DZ-LIC-2026-1102', type: 'text' },
        {
          key: 'licensor',
          label: '授权方',
          default: '知数先锋技术(开发测试专用)有限公司',
          type: 'text',
        },
        {
          key: 'licensee',
          label: '被授权方',
          default: '杭州知数云测信息技术有限公司（测试主体）',
          type: 'text',
        },
        { key: 'instance_limit', label: '授权运行实例上限 (节点)', default: '500', type: 'number' },
        {
          key: 'license_key_prefix',
          label: 'License 密匙前缀',
          default: 'DZN-ENT-QA26-X77',
          type: 'text',
        },
        {
          key: 'annual_maintenance_fee',
          label: '年度维护技术支持费 (元)',
          default: '96000.00',
          type: 'amount',
        },
        { key: 'fee_words', label: '维保费大写', default: '玖万陆仟元整', type: 'text' },
        {
          key: 'support_channel',
          label: '技术支持渠道',
          default: '专属企业微信群 + 7×24小时专属技术工程师支持',
          type: 'text',
        },
        { key: 'effective_date', label: '授权生效日', default: '2026-09-01', type: 'date' },
      ],
      content: [
        '# 商业数据库管理套件授权与持续维保续约协议',
        '',
        '**合同编号**：{{contract_no}}  ',
        '**生效日期**：{{effective_date}}  ',
        '',
        '### 当事人',
        '- **软件著作权/授权方**：{{licensor}}',
        '- **终端企业用户/被授权方**：{{licensee}}',
        '',
        '### 第一条 软件商业授权许可',
        '1. 授权方授予被授权方非排他性、不可转让的企业级商业使用权，授权安装与运行节点实例上限为 **{{instance_limit}}** 个。',
        '2. 授权凭证：被授权方使用授权码 **{{license_key_prefix}}**** 进行企业多端集中鉴权激活。',
        '',
        '### 第二条 维保服务与升级支持',
        '1. 在本协议有效维保期内，被授权方享有所有次版本及补丁版本（Minor/Patch Release）的无缝免费平滑升级权益。',
        '2. 授权方提供 **{{support_channel}}** 的维保响应服务。',
        '3. 年度维保费为人民币 **￥{{annual_maintenance_fee}} 元**（大写：**{{fee_words}}**）。',
      ].join('\n'),
    },
  ];

  // 辅助函数：人民币金额转大写
  function amountToWords(n) {
    var fraction = ['角', '分'];
    var digit = ['零', '壹', '贰', '叁', '肆', '伍', '陆', '柒', '捌', '玖'];
    var unit = [
      ['元', '万', '亿'],
      ['', '拾', '佰', '仟'],
    ];
    var num = Math.abs(parseFloat(n));
    if (isNaN(num)) return '';

    var head = num < 0 ? '欠' : '';
    var s = '';

    var dec = Math.floor((num * 100) % 100);
    var d1 = Math.floor(dec / 10);
    var d2 = dec % 10;
    if (d1 > 0 || d2 > 0) {
      if (d1 > 0) s += digit[d1] + fraction[0];
      if (d2 > 0) s += digit[d2] + fraction[1];
    } else {
      s += '整';
    }

    var integerPart = Math.floor(num);
    if (integerPart === 0) {
      return s === '整' ? '零元整' : s;
    }

    for (var i = 0; i < unit[0].length && integerPart > 0; i++) {
      var p = '';
      for (var j = 0; j < unit[1].length && integerPart > 0; j++) {
        p = digit[integerPart % 10] + unit[1][j] + p;
        integerPart = Math.floor(integerPart / 10);
      }
      s = p.replace(/(零.)*零$/, '').replace(/^$/, '零') + unit[0][i] + s;
    }

    return (
      head +
      s
        .replace(/(零.)*零元/, '元')
        .replace(/(零.)+/g, '零')
        .replace(/^整$/, '零元整')
    );
  }

  // 随机生成测试公司名
  var TEST_CITIES = ['杭州', '北京', '上海', '深圳', '广州', '成都', '武汉', '南京'];
  var TEST_PREFIXES = [
    '知数',
    '极客',
    '云海',
    '星辰',
    '神策',
    '数舟',
    '全栈',
    '数聚',
    '天穹',
    '先锋',
  ];
  var TEST_INDUSTRIES = ['智能科技', '软件技术', '数据引擎', '网络互联', '信息工程', '云原生计算'];
  var TEST_COMPANY_TYPES = ['股份有限公司', '科技有限公司', '技术研究院有限公司'];

  function generateRandomCompanyName() {
    var city = TEST_CITIES[Math.floor(Math.random() * TEST_CITIES.length)];
    var pfx = TEST_PREFIXES[Math.floor(Math.random() * TEST_PREFIXES.length)];
    var ind = TEST_INDUSTRIES[Math.floor(Math.random() * TEST_INDUSTRIES.length)];
    var typ = TEST_COMPANY_TYPES[Math.floor(Math.random() * TEST_COMPANY_TYPES.length)];
    return city + pfx + ind + typ;
  }

  // 随机生成联系人姓名
  var SURNAMES = ['张', '李', '王', '赵', '钱', '孙', '周', '吴', '郑', '陈', '林', '黄'];
  var GIVENNAMES = [
    '伟',
    '芳',
    '娜',
    '敏',
    '静',
    '丽',
    '强',
    '磊',
    '军',
    '洋',
    '勇',
    '杰',
    '涛',
    '明',
    '超',
    '子健',
    '思睿',
  ];
  function generateRandomName() {
    var s = SURNAMES[Math.floor(Math.random() * SURNAMES.length)];
    var g = GIVENNAMES[Math.floor(Math.random() * GIVENNAMES.length)];
    return s + g;
  }

  // 随机生成手机号
  function generateRandomPhone() {
    var prefix = ['138', '139', '150', '186', '188', '177', '199'][Math.floor(Math.random() * 7)];
    var rest = String(Math.floor(10000000 + Math.random() * 90000000));
    return prefix + rest;
  }

  // 随机生成合同编号
  function generateRandomContractNo(envCode, tplCode) {
    var now = new Date();
    var ymd =
      now.getFullYear() + ('0' + (now.getMonth() + 1)).slice(-2) + ('0' + now.getDate()).slice(-2);
    var rand = String(Math.floor(100 + Math.random() * 900));
    return (envCode || 'QA') + '-' + (tplCode || 'HT') + '-' + ymd + '-' + rand;
  }

  // 导出到全局
  window.ContractTemplatesData = {
    ENVIRONMENTS: ENVIRONMENTS,
    TEMPLATES: TEMPLATES,
    amountToWords: amountToWords,
    generateRandomCompanyName: generateRandomCompanyName,
    generateRandomName: generateRandomName,
    generateRandomPhone: generateRandomPhone,
    generateRandomContractNo: generateRandomContractNo,
  };
})();
