# ATHENA fleet relay

A small Cloudflare Worker that lets ATHENA stations of one group exchange messages over the
internet. It is a mailbox: stations post messages they have already sealed with their group key,
and read back what the others posted. The relay cannot read anything it stores and holds no keys.

The local network and radio keep working without it. It is an extra path for stations that are
far apart but both online.

## What the relay sees

- A room name and a room password, both derived from the group key on each station. Stations
  with the same group key end up in the same room; nobody else can find or open it.
- Sealed messages (ChaCha20-Poly1305), with the time they arrived.
- The IP address of each station that connects, as with any website.

It keeps at most 2,000 messages per room and drops anything older than 14 days.

## Deploy

You need a Cloudflare account (the free plan is enough for a small group) and Node.js.

```bash
cd cloud/fleet-relay
npx wrangler@4 login
npx wrangler@4 deploy
```

Wrangler prints the address, such as `https://athena-fleet-relay.<your-subdomain>.workers.dev`.

## Connect stations

On each station:

1. Field Ops, Other stations: set the same group key on every station (generate it on one,
   copy it to the others by hand or courier bundle; never send it over the internet).
2. Library, Vault, Fleet link: paste the relay address and switch the fleet link on.

## Test

```bash
node --test cloud/fleet-relay/test/relay.test.mjs
```
