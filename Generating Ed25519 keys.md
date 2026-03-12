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