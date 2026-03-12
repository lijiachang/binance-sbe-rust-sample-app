Generating Ed25519 keys
Option 1: OpenSSL (recommended)


# Generate private key (PKCS#8 PEM format)
openssl genpkey -algorithm ed25519 -out binance_ed25519_private.pem
# Extract public key
openssl pkey -in binance_ed25519_private.pem -pubout -out binance_ed25519_public.pem



Option 2: Binance Key Generator

Download the Binance Asymmetric Key Generator from the releases page and run it to generate a keypair.
https://github.com/binance/asymmetric-key-generator



Registering with Binance

Log in to Binance and go to Profile → API Management
Click Create API and select Self-generated
Paste the contents of your public key file (including the -----BEGIN PUBLIC KEY----- header/footer)
Configure permissions (Enable Spot & Margin Trading, etc.)


另外 bookticker 行情采集 这个程序根本不需要私钥。

代码中唯一涉及身份验证的地方就是这一段：
rust

req.headers_mut().insert(
    "X-MBX-APIKEY",
    cfg.api_key.parse().context("API key 不是合法 header value")?,
);

它只是把 API Key（一个公开的字符串标识符）放进了 HTTP 请求头，没有任何签名操作。

这和 Binance SBE 文档的说明完全一致：

    "打开连接时，请将您的 API Key 放在 X-MBX-APIKEY 标头中。时间戳和签名不是必需的。"

因为 SBE 市场数据流是公开市场数据，不涉及下单、查余额等敏感操作，所以只需要 API Key 做身份识别，不需要用私钥进行签名。

在服务器上你只需要做一件事：

把 API Key 传给程序，二选一：
bash

# 方式一：环境变量
export BINANCE_ED25519_API_KEY="你的APIKey字符串"
./sbe-bookticker-app --symbol btcusdt

# 方式二：命令行参数
./sbe-bookticker-app --api-key "你的APIKey字符串" --symbol btcusdt

私钥文件完全不需要复制到服务器上（至少对这个程序而言不需要）。私钥只有在你需要对请求进行签名时才会用到，比如调用交易接口、账户接口等。如果你以后扩展程序加入签名功能，那时候才需要考虑私钥的存放。