/** Browser regression using Preview IPC fixtures, without native Whisper inference. */
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
(async () => {
 const browser = await chromium.launch({executablePath:process.env.CHROMIUM_PATH || undefined, args:['--no-sandbox']});
 try {
 const flag = process.argv.indexOf('--base-url');
 const baseUrl = flag >= 0 ? process.argv[flag + 1] : process.env.PREVIEW_URL || 'http://localhost:3118';
 const page = await browser.newPage({viewport:{width:1440,height:1000}, locale:'en-US'});
 page.on('pageerror', e => console.log('PAGEERROR',e.message));
 await page.goto(`${baseUrl}/settings?section=transcription`,{waitUntil:'networkidle'});
 const language = page.getByRole('combobox',{name:'Default Whisper language'});
 await language.waitFor();
 assert.match(await language.innerText(),/Original Language/);
 assert.match(await page.locator('body').innerText(),/Selected for live/);
 await language.click(); await page.getByRole('option',{name:/^Russian/}).click();
 assert.equal(await page.evaluate(()=>localStorage.getItem('primaryLanguage')),'ru');
 await page.reload({waitUntil:'networkidle'});
 assert.match(await language.innerText(),/Russian/);
 await language.scrollIntoViewIfNeeded();
 if (process.env.PREVIEW_SCREENSHOT) await page.screenshot({path:process.env.PREVIEW_SCREENSHOT});
 await page.goto(baseUrl,{waitUntil:'networkidle'});
 await page.evaluate(async()=>{
  const original=window.__TAURI_INTERNALS__.invoke;
  // Preview has no installed Whisper model or persisted post-call config.
  window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
   if(command==='api_get_post_call_transcript_config') return {provider:'whisper',model:'large-v3'};
   if(command==='whisper_get_available_models') return [{name:'large-v3',status:'Available',size_mb:3000}];
   if(command==='start_retranscription_command'){window.__retranscriptionArgs=args;return null;}
   return original(command,args);
  };
 });
 await page.getByRole('button',{name:'Start recording',exact:true}).click();
 await page.getByRole('button',{name:'Pause recording',exact:true}).click();
 await page.getByRole('button',{name:'Stop recording',exact:true}).click();
 await page.waitForURL(/meeting-details/,{timeout:25000});
 await page.getByRole('button',{name:'Continue',exact:true}).click();
 await page.waitForFunction(()=>!!window.__retranscriptionArgs);
 const args=await page.evaluate(()=>window.__retranscriptionArgs);
 assert.equal(args.language,'ru');assert.equal(args.provider,'whisper');assert.equal(args.model,'large-v3');
 console.log('PASS: default Auto with English locale; Russian available with live Parakeet, persists reload, reaches automatic Whisper post-call IPC',args);
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
