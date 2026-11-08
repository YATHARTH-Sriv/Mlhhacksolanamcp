# Hover MCP Makes Talking to Solana Simple 

## For non crypto audience blockchain still feels a bit too tech savy 

### most of the people do not attempt to use it Hover MCP makes it easy with integrating with thier daily tool chatgpt or codex

## Project Demo

[![Watch the Hover MCP demo on YouTube](https://img.youtube.com/vi/vJbglbGF7_o/hqdefault.jpg)](https://www.youtube.com/watch?v=vJbglbGF7_o)

[Watch the full demo on YouTube](https://www.youtube.com/watch?v=vJbglbGF7_o)

## Overview of Tools Available as of right now 

1. Show connected wallet address

2. Show portfolio with prices ( although everything show in demo and in this project is on surfpool but we use Jupiter price api to show what mainnet scenario would look like )

3. Quote Swaps with just Natural language Convert 10 SOL to USDC

4. Send money SOL or any other TOKEN 2022 token to anyone  
    - we do not store any keys we just build a txn user signs on the connected wallet 

5. Pay any bill ( which is in USD or SOL) with any token of you choice 
    - obviously as per the tokens present in your portfolio

---
---

## How to Run this Project on Local Device

### 1. Things you need first

- Node.js 22 or newer and npm
- Rust stable and Cargo
- [Surfpool](https://www.surfpool.run/)
- Phantom browser extension with a wallet
- A [Jupiter API key](https://portal.jup.ag/) with access to Price, Tokens and Swap APIs
- ChatGPT with access to custom MCP plugins

Clone or download this repo and open it in your editor. All the commands below start from the main project folder unless mentioned otherwise.

### 2. Start Surfpool and add test tokens

Copy the public wallet address from Phantom. Replace `YOUR_PHANTOM_WALLET_ADDRESS` below with that address, not a private key.

```bash
surfpool start --no-deploy --airdrop YOUR_PHANTOM_WALLET_ADDRESS --airdrop-amount 100000000000
```

This starts the local RPC at `http://127.0.0.1:8899` and adds 100 test SOL to your wallet. Keep this terminal running.

Open Surfpool Studio at `http://127.0.0.1:18488`. Use its faucet to add test USDC or JUP to the same Phantom wallet if you want to test token payments as well.

These are the mainnet mint addresses used in the demo:

- USDC: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
- JUP: `JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN`

These are test balances in Surfpool. 

### 3. Start ngrok

ChatGPT needs a public HTTPS URL to reach the local server. We expose the frontend on port 3000 because it also forwards MCP and API requests to the backend.

In a new terminal run:

```bash
ngrok http 3000
```

Copy the HTTPS forwarding URL, for example `https://your-domain.ngrok-free.app`. Keep this terminal running too. The frontend will start in step 5, so the tunnel might show an error until then.

### 4. Add the env variables in both folders

Create a `.env` file inside `backend`:

```dotenv
RPC_URL=http://127.0.0.1:8899
APP_ORIGIN=https://your-domain.ngrok-free.app
FRONTEND_ORIGIN=https://your-domain.ngrok-free.app
OAUTH_CLIENT_ID=hover-local
OAUTH_REDIRECT_URI=https://chatgpt.com/connector_platform_oauth_redirect
SOLANA_NETWORK=surfpool
JUPITER_API_KEY=YOUR_JUPITER_API_KEY
PORT=8000
```

Create a `.env` file inside `frontend`:

```dotenv
FRONTEND_ORIGIN=https://your-domain.ngrok-free.app
BACKEND_INTERNAL_URL=http://127.0.0.1:8000
```

Replace `https://your-domain.ngrok-free.app` in all three places with your own ngrok URL. Use the same HTTPS URL without a trailing `/`. Add your real Jupiter API key only in the backend file.

`OAUTH_REDIRECT_URI` is the callback back to ChatGPT, not your ngrok URL. The value above is the one used in this demo. If the plugin setup shows a different callback, copy that exact value here and restart the backend.

No wallet private key, seed phrase. Do not commit your `.env` files.

### 5. Run the backend and frontend

Open a new terminal for the backend:

```bash
cd backend
cargo run
```

Open another terminal from the main project folder for the frontend:

```bash
cd frontend
npm install
npm run dev
```

The frontend runs at `http://localhost:3000`. Open your **ngrok HTTPS URL** in the browser with Phantom installed to use the public app.

You should now have four terminals running: Surfpool, ngrok, backend and frontend.

To check that the public URL reaches the backend, open:

```text
https://your-domain.ngrok-free.app/health/live
https://your-domain.ngrok-free.app/.well-known/oauth-authorization-server
```

The second URL should return OAuth configuration with your ngrok URL in it.

### 6. Add Hover in ChatGPT

Open the [ChatGPT Plugins page](https://chatgpt.com/plugins), click the **+** button and choose **Add custom MCP server**. If your account uses the older Apps interface, enable Developer mode in its advanced settings and add a custom app there.

Fill in these details:

| Field | What to add |
| --- | --- |
| Name | Hover MCP |
| Description | Talk to your Solana wallet, check your portfolio, prepare payments and preview swap quotes. |
| Connection | Public URL |
| MCP server URL | `https://your-domain.ngrok-free.app/mcp` |
| Authentication | OAuth |
| OAuth client ID | `hover-local` |
| OAuth client secret | Leave empty; this backend uses public-client OAuth with PKCE. |

If asked for scopes, use `identity:read portfolio:read transactions:write`. If asked for the token endpoint authentication method, choose `none`. Use the predefined/manual OAuth client option if it asks how to register the client.

Review the warning and create the plugin. Connect it, open the Hover login page in the browser with Phantom, and use the same wallet you funded in Surfpool. Review the permissions and sign the login message.

Once connected, start a new chat, type `@` and select **Hover MCP**. You should have seven tools: `whoami`, `portfolio`, `prepare_transfer`, `transfer_status`, `quote_bill_payment`, `prepare_bill_payment` and `quote_swap`.


### 7. Try it out

```text
What is my connected wallet address?

Show my portfolio with prices.

Show a quote for swapping 1 SOL to USDC with 0.5% slippage.

Send 1 SOL to <recipient wallet address>.
```

For a payment, open the review link returned by Hover, check the details, tick the confirmation box and sign in Phantom. Hover submits the signed transaction to Surfpool. Then ask for the transaction status and your updated portfolio.

For a bill, attach an image or PDF and ask `Create a payment for this bill`. ChatGPT reads it and asks you to confirm the total and currency. Choose a payment token and give the recipient wallet address when asked, then review and sign through the same flow.

Swaps are quote-only for now. They do not open Phantom or change any balances.

---
---