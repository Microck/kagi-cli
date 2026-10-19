import { readFileSync,readdirSync } from "node:fs";
import { join } from "node:path";
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
  return [{route:"/"+path.slice(root.length+1).replace(/\.mdx?$/,"").replace(/(^|\/)index$/,""),title}];
 });
}
for(const page of pages())test(`documentation ${page.route}`,async({app,screen})=>{
 await app.open(page.route);
 await expect(screen.getByRole("heading",{name:`${page.title} Link to section`,exact:true})).toBeVisible();
});
