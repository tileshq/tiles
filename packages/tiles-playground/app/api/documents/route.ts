import { sql } from '@vercel/postgres';
import { NextResponse } from 'next/server';
import { nanoid } from 'nanoid';

// POST /api/documents
export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { 
      id = nanoid(), // Use provided ID or generate new one
      title = 'Untitled', 
      description = '', 
      content 
    } = body;

    // Ensure content is a string if it's an object
    const contentStr = typeof content === 'string' ? content : JSON.stringify(content);

    const { rows } = await sql`
      INSERT INTO documents (id, title, description, content)
      VALUES (${id}, ${title}, ${description}, ${contentStr}::jsonb)
      RETURNING *
    `;

    return NextResponse.json(rows[0]);
  } catch (error) {
    console.error('Error creating document:', error);
    return NextResponse.json(
      { error: 'Failed to create document' },
      { status: 500 }
    );
  }
} 