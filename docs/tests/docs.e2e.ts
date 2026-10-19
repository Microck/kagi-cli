import { readFileSync,readdirSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { test } from "@e2e-dev/web";
import { expect } from "e2e";
const root="content/docs";
function pages(directory=root):{route:string;title:string}[]{
 return readdirSync(directory,{withFileTypes:true}).flatMap(entry=>{
  const path=join(directory,entry.name);
  if(entry.isDirectory())return entry.name==="reviews"?[]:pages(path);
  if(!/\.mdx?$/.test(entry.name))return [];
  // These sites render the Markdown H1, not the navigation frontmatter title.
  const title=readFileSync(path,"utf8").match(/^# (.+)$/m)?.[1]
    ?.replace(/[`*_]/g, "").replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
  if(!title)throw new Error(`Missing title: ${path}`);
  return [{route:"/"+relative(root,path).split(sep).join("/").replace(/\.mdx?$/,"").replace(/(^|\/)index$/,""),title}];
 });
}
for(const page of pages())test(`documentation ${page.route}`,async({app,screen,browser})=>{
 await browser.addInitScript(() => {
  const errors: string[] = [];
  Object.assign(window, { __qaErrors: errors });
  window.addEventListener('error', event => errors.push(event.message));
  window.addEventListener('unhandledrejection', event => errors.push(String(event.reason)));
  const originalError = console.error;
  console.error = (...args: unknown[]) => {
    errors.push(args.map(String).join(" "));
    originalError.apply(console, args);
  };
 });
 await app.open(page.route);
 await expect(screen.getByRole("heading",{name:`${page.title} Link to section`,exact:true,level:1})).toBeVisible();
 await screen.getByRole("button", {name:"Open Search",exact:true}).click();
 await expect(screen.getByRole("dialog")).toBeVisible();
 await browser.keyboard.press("Escape");
 await expect(screen.getByRole("dialog")).not.toBeVisible();
 expect(await browser.evaluate(() => (window as unknown as {__qaErrors:string[]}).__qaErrors)).toEqual([]);
});
