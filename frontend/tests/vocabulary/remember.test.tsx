import React from 'react';
import { act,create,type ReactTestRenderer } from 'react-test-renderer';
import { expect,mock,test } from 'bun:test';
let release:()=>void=()=>{}, confirmations=0, corrected='Saved correction independent of vocabulary';
mock.module('@tauri-apps/api/core',()=>({invoke:async(name:string,args:any)=>{
 if(name!=='api_confirm_vocabulary_term') throw new Error(`Unexpected destructive command ${name}`);
 expect(args).toEqual({term:'Ёлка API'}); confirmations++;
 await new Promise<void>(resolve=>{release=resolve;});
 return 'Ёлка API';
}}));
mock.module('@/components/ui/button',()=>({Button:({children,...props}:any)=><button {...props}>{children}</button>}));
const {RememberTerm}=await import('../../src/components/meeting/RememberTerm');
test('double click is bounded, success is explicit, and dismiss never mutates correction',async()=>{
 let root:ReactTestRenderer;
 await act(async()=>{root=create(<RememberTerm correctedText={corrected} onDismiss={()=>{}}/>);});
 const button=()=>root.root.findAllByType('button').find(n=>n.props.children==='Запомнить')!;
 expect(button().props.disabled).toBe(true);
 await act(async()=>root.root.findByType('input').props.onChange({target:{value:' Ёлка API '}}));
 await act(async()=>{button().props.onClick();button().props.onClick();});
 expect(confirmations).toBe(1);
 expect(button().props.disabled).toBe(true);
 await act(async()=>release());
 expect(JSON.stringify(root!.toJSON())).toContain('Термин сохранён');
 expect(corrected).toBe('Saved correction independent of vocabulary');
 expect(root!.root.findAllByType('button').some(n=>n.props.children==='Запомнить')).toBe(false);
 await act(async()=>root!.unmount());
});
