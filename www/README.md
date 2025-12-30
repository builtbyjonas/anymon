# Anymon — minimal Express app for www/

- Start locally: `npm install` then `npm start` (or `npm run dev` with `nodemon`).
- The app redirects `/` to the repository website (can be overridden with `REPOSITORY_URL`).
- The routes `/install.ps1` and `/install.sh` return the contents of those files as plain text.

Vercel deploy: this folder contains `vercel.json` which builds `server.js` with `@vercel/node`.

Example local run:
```
cd www
npm install
npm start
# open http://localhost:3000/
```
